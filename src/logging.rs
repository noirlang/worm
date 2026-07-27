//! Uygulama günlüklerini dosyaya ve bellekteki kısa kuyruğa yazar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
/// Günlük seviyelerini sıralı önem derecesiyle temsil eder.
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Günlük seviyesini dosyaya yazılacak kısa etikete çevirir.
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
/// Developer mod penceresinde gösterilen uygulama geneli günlük satırıdır.
pub struct RuntimeLogEntry {
    pub seq: u64,
    pub timestamp: String,
    pub level: String,
    pub scope: String,
    pub message: String,
    pub thread: String,
}

/// Bellekteki runtime log kuyruğunu ve opsiyonel dosya çıktısını tutar.
struct RuntimeLogStore {
    entries: Vec<RuntimeLogEntry>,
    file_path: Option<PathBuf>,
    file: Option<File>,
}

static NEXT_RUNTIME_LOG_SEQ: AtomicU64 = AtomicU64::new(1);

/// Uygulama geneli runtime log deposunu döndürür.
fn runtime_log_store() -> &'static Mutex<RuntimeLogStore> {
    static STORE: OnceLock<Mutex<RuntimeLogStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        let (file_path, file) = open_runtime_log_file()
            .map(|(path, file)| (Some(path), Some(file)))
            .unwrap_or((None, None));
        Mutex::new(RuntimeLogStore {
            entries: Vec::new(),
            file_path,
            file,
        })
    })
}

/// Runtime log dosyasını kullanıcı Amele klasörü altında açar.
fn open_runtime_log_file() -> Option<(PathBuf, File)> {
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Amele")
        .join("gunlukler");
    fs::create_dir_all(&base).ok()?;
    let path = base.join(format!(
        "runtime_{}.log",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some((path, file))
}

/// Developer mod için uygulama geneli log satırı yazar.
pub fn runtime_log(level: LogLevel, scope: impl AsRef<str>, message: impl AsRef<str>) {
    let message = message.as_ref().trim();
    if message.is_empty() {
        return;
    }

    let entry = RuntimeLogEntry {
        seq: NEXT_RUNTIME_LOG_SEQ.fetch_add(1, Ordering::SeqCst),
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        level: level.as_str().to_string(),
        scope: scope.as_ref().to_string(),
        message: message.to_string(),
        thread: std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string(),
    };
    let file_line = format!(
        "[{}] | {} | {} | {} | {}\n",
        entry.timestamp, entry.level, entry.thread, entry.scope, entry.message
    );

    if level >= LogLevel::Warn {
        eprintln!(
            "[AMELE {}] {}: {}",
            level.as_str(),
            entry.scope,
            entry.message
        );
    }

    if let Ok(mut store) = runtime_log_store().lock() {
        if let Some(file) = store.file.as_mut() {
            let _ = file.write_all(file_line.as_bytes());
            let _ = file.flush();
        }
        store.entries.push(entry);
        let overflow = store.entries.len().saturating_sub(2000);
        if overflow > 0 {
            store.entries.drain(0..overflow);
        }
    }
}

/// Son runtime log satırlarını kopya olarak döndürür.
pub fn runtime_logs(limit: usize) -> Vec<RuntimeLogEntry> {
    let limit = limit.clamp(1, 2000);
    runtime_log_store()
        .lock()
        .map(|store| {
            let start = store.entries.len().saturating_sub(limit);
            store.entries[start..].to_vec()
        })
        .unwrap_or_default()
}

/// Runtime log dosya yolunu döndürür.
pub fn runtime_log_file_path() -> Option<PathBuf> {
    runtime_log_store()
        .lock()
        .ok()
        .and_then(|store| store.file_path.clone())
}

#[derive(Clone)]
/// Vaka günlük dosyasına thread-safe satır yazan logger nesnesidir.
pub struct Logger {
    case_name: String,
    log_dir: PathBuf,
    active_file: PathBuf,
    min_level: LogLevel,
    file: Arc<Mutex<File>>,
}

impl Logger {
    /// Vaka için yeni günlük dosyası açar.
    pub fn start(case_name: impl AsRef<str>, log_dir: impl AsRef<Path>) -> AmeleResult<Self> {
        let case_name = case_name.as_ref().to_string();
        let log_dir = log_dir.as_ref().to_path_buf();
        fs::create_dir_all(&log_dir).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Gunluk klasoru olusturulamadi", err)
        })?;

        let file_name = format!("{}_{}.log", case_name, Local::now().format("%Y%m%d_%H%M%S"));
        let active_file = log_dir.join(file_name);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active_file)
            .map_err(|err| {
                AmeleError::io(HataKodu::DosyaAcilamadi, "Gunluk dosyasi acilamadi", err)
            })?;

        let logger = Self {
            case_name,
            log_dir,
            active_file,
            min_level: LogLevel::Debug,
            file: Arc::new(Mutex::new(file)),
        };
        logger.info(format!(
            "Gunluk sistemi baslatildi: {}",
            logger.active_file.display()
        ));
        Ok(logger)
    }

    /// Aktif günlük dosyası yolunu döndürür.
    pub fn active_file(&self) -> &Path {
        &self.active_file
    }

    /// Logger'ın bağlı olduğu vaka adını döndürür.
    pub fn case_name(&self) -> &str {
        &self.case_name
    }

    /// Günlük klasörü yolunu döndürür.
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    /// Seviyeye göre filtreleyip günlük satırını dosyaya yazar.
    pub fn log(&self, level: LogLevel, message: impl AsRef<str>) {
        if level < self.min_level {
            return;
        }

        let line = format!(
            "[{}] | {} | {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            level.as_str(),
            message.as_ref()
        );

        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }

        runtime_log(
            level,
            format!("case:{}", self.case_name),
            message.as_ref().to_string(),
        );
    }

    /// Sistem kaynaklı mesajı özel etiketle yazar.
    pub fn system(&self, level: LogLevel, message: impl AsRef<str>) {
        self.log(level, format!("[SISTEM] {}", message.as_ref()));
    }

    /// Bilgi seviyesinde günlük yazar.
    pub fn info(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Info, message);
    }

    /// Uyarı seviyesinde günlük yazar.
    pub fn warn(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Warn, message);
    }

    /// Hata seviyesinde günlük yazar.
    pub fn error(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Error, message);
    }

    /// Debug seviyesinde günlük yazar.
    pub fn debug(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Debug, message);
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        self.info("Gunluk sistemi kapatiliyor");
    }
}
