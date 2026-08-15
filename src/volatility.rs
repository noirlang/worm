//! Volatility3 entegrasyonu, sembol kontrolü ve RAM proses analizi işlemlerini yürütür.
use crate::logging::{LogLevel, runtime_log};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

const LINUX_BANNER_NEEDLE: &[u8] = b"Linux version ";
const LINUX_BANNER_MAX_LEN: usize = 320;
const LINUX_BANNER_SCAN_CHUNK: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Volatility proses çıktısını arayüzün kullandığı ortak formata indirger.
pub struct VolatilityProcess {
    pub pid: i64,
    pub ppid: i64,
    pub name: String,
    pub offset: String,
    pub extra_info: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// RAM analizi öncesi Volatility, sembol ve Linux banner durumunu raporlar.
pub struct VolatilityPreflight {
    pub ready: bool,
    pub vol_py: Option<String>,
    pub symbol_dirs: Vec<String>,
    pub symbol_count: usize,
    pub linux_symbol_count: usize,
    pub banners: Vec<String>,
    pub matching_symbols: Vec<String>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Ortam değişkenleri, çalışma klasörü ve paket içi yollardan vol.py dosyasını bulur.
pub fn locate_vol_py() -> Option<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(value) = env::var("AMELE_VOLATILITY3_PATH") {
        push_volatility_candidate(&mut paths, PathBuf::from(value));
    }
    if let Ok(value) = env::var("VOLATILITY3_PATH") {
        push_volatility_candidate(&mut paths, PathBuf::from(value));
    }

    if let Ok(cwd) = env::current_dir() {
        push_volatility_candidate(&mut paths, cwd.join("volatility3"));
        push_volatility_candidate(&mut paths, cwd.join("vendor/volatility3"));
        push_volatility_candidate(&mut paths, cwd.join("../volatility3"));
        push_volatility_candidate(&mut paths, cwd.join("../vendor/volatility3"));
        push_volatility_candidate(&mut paths, cwd.join("../../volatility3"));
        push_volatility_candidate(&mut paths, cwd.join("../../vendor/volatility3"));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        push_volatility_candidate(&mut paths, dir.join("volatility3"));
        push_volatility_candidate(&mut paths, dir.join("vendor/volatility3"));
        push_volatility_candidate(&mut paths, dir.join("../share/amele/vendor/volatility3"));
        push_volatility_candidate(&mut paths, dir.join("../volatility3"));
        push_volatility_candidate(&mut paths, dir.join("../vendor/volatility3"));
        push_volatility_candidate(&mut paths, dir.join("../../volatility3"));
        push_volatility_candidate(&mut paths, dir.join("../../vendor/volatility3"));
    }

    push_volatility_candidate(
        &mut paths,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/volatility3"),
    );

    paths
        .into_iter()
        .find(|path| path.exists())
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
}

/// Paketlenen yardımcı Python worker dosyasının yolunu bulur.
pub fn locate_worker_py() -> Option<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(value) = env::var("AMELE_VOLATILITY_WORKER_PATH") {
        paths.push(PathBuf::from(value));
    }

    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join("tools/amele_volatility_worker.py"));
        paths.push(cwd.join("../tools/amele_volatility_worker.py"));
        paths.push(cwd.join("../../tools/amele_volatility_worker.py"));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join("tools/amele_volatility_worker.py"));
        paths.push(dir.join("../tools/amele_volatility_worker.py"));
        paths.push(dir.join("../share/amele/tools/amele_volatility_worker.py"));
        paths.push(dir.join("../../tools/amele_volatility_worker.py"));
    }

    paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/amele_volatility_worker.py"));

    paths
        .into_iter()
        .find(|path| path.exists())
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
}

/// Klasör veya doğrudan vol.py verilmiş yolu aday listesine ekler.
fn push_volatility_candidate(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.file_name().is_some_and(|name| name == "vol.py") {
        paths.push(path);
    } else {
        paths.push(path.join("vol.py"));
    }
}

/// Volatility pluginini logsuz ve varsayılan sembol dizinleriyle çalıştırır.
pub fn run_volatility_plugin(
    file_path: &Path,
    plugin: &str,
    extra_args: &[&str],
) -> Result<Value, String> {
    run_volatility_plugin_logged(file_path, plugin, extra_args, None)
}

/// Volatility pluginini canlı konsol loglarıyla çalıştırır.
pub fn run_volatility_plugin_logged(
    file_path: &Path,
    plugin: &str,
    extra_args: &[&str],
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Value, String> {
    run_volatility_plugin_logged_with_symbol_dir(file_path, plugin, extra_args, None, log)
}

/// Volatility pluginini seçilen sembol klasörü ve canlı log desteğiyle yürütür.
pub fn run_volatility_plugin_logged_with_symbol_dir(
    file_path: &Path,
    plugin: &str,
    extra_args: &[&str],
    symbol_dir: Option<&Path>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Value, String> {
    if let Some(worker_py) = locate_worker_py() {
        return run_worker_plugin_logged(
            file_path, plugin, extra_args, symbol_dir, &worker_py, log,
        );
    }

    let Some(vol_py) = locate_vol_py() else {
        return Err(
            "Volatility3 vol.py bulunamadı. AMELE_VOLATILITY3_PATH ile vol.py/volatility3 klasörünü veya AMELE_VOLATILITY_WORKER_PATH ile worker yolunu belirtin."
                .to_string(),
        );
    };

    let vol_dir = vol_py.parent().unwrap_or(Path::new("."));
    let mut args = vec![vol_py.to_string_lossy().into_owned()];
    if let Some(symbols_arg) = symbol_dirs_arg(symbol_dir) {
        args.push("-s".to_string());
        args.push(symbols_arg);
    }
    if let Some(cache_path) = volatility_cache_path() {
        args.push("--cache-path".to_string());
        args.push(cache_path.to_string_lossy().into_owned());
    }
    args.extend([
        "-q".to_string(),
        "-f".to_string(),
        file_path.to_string_lossy().into_owned(),
        "-r".to_string(),
        "json".to_string(),
        plugin.to_string(),
    ]);
    args.extend(extra_args.iter().map(|arg| arg.to_string()));

    if let Some(log) = &log {
        log(format!("Volatility3 çalışıyor: {plugin}"));
        log(format!("vol.py: {}", vol_py.display()));
        log(format!("RAM imajı: {}", file_path.display()));
        let symbol_dirs = configured_symbol_dirs(symbol_dir);
        if !symbol_dirs.is_empty() {
            log(format!(
                "Volatility symbol dizinleri: {}",
                symbol_dirs
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }

    run_volatility_command(vol_dir, args, plugin, log)
}

/// Python worker varsa Volatility çağrısını izole yardımcı süreç üzerinden yapar.
fn run_worker_plugin_logged(
    file_path: &Path,
    plugin: &str,
    extra_args: &[&str],
    symbol_dir: Option<&Path>,
    worker_py: &Path,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Value, String> {
    let mut args = vec![
        worker_py.to_string_lossy().into_owned(),
        "plugin".to_string(),
        "--file".to_string(),
        file_path.to_string_lossy().into_owned(),
    ];
    if let Some(vol_py) = locate_vol_py() {
        args.push("--vol-py".to_string());
        args.push(vol_py.to_string_lossy().into_owned());
    }
    for dir in configured_symbol_dirs(symbol_dir) {
        args.push("--symbol-dir".to_string());
        args.push(dir.to_string_lossy().into_owned());
    }
    args.push("--".to_string());
    args.push(plugin.to_string());
    args.extend(extra_args.iter().map(|arg| arg.to_string()));

    if let Some(log) = &log {
        log(format!(
            "Volatility worker çalışıyor: {}",
            worker_py.display()
        ));
        log(format!("Volatility3 plugin: {plugin}"));
    }

    run_volatility_command(Path::new("."), args, plugin, log)
}

/// Python sürecini başlatır, stdout/stderr akışlarını toplar ve JSON sonucu parse eder.
fn run_volatility_command(
    workdir: &Path,
    args: Vec<String>,
    plugin: &str,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Value, String> {
    let args_debug = args.join(" ");
    runtime_log(
        LogLevel::Info,
        "volatility",
        format!(
            "Volatility3 sureci baslatiliyor. Cwd: {}, Komut: python3 {}",
            workdir.display(),
            args_debug
        ),
    );

    let mut child = Command::new("python3")
        .args(&args)
        .current_dir(workdir)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            let err_msg = format!("python3 çalıştırılamadı: {err}");
            runtime_log(LogLevel::Error, "volatility", &err_msg);
            err_msg
        })?;

    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));
    let stdout_thread = child
        .stdout
        .take()
        .map(|stdout| spawn_reader(stdout, "stdout", stdout_buffer.clone(), log.clone(), false));
    let stderr_thread = child
        .stderr
        .take()
        .map(|stderr| spawn_reader(stderr, "stderr", stderr_buffer.clone(), log.clone(), true));

    let status = child.wait().map_err(|err| {
        let err_msg = format!("Volatility3 süreci beklenemedi: {err}");
        runtime_log(LogLevel::Error, "volatility", &err_msg);
        err_msg
    })?;
    if let Some(handle) = stdout_thread {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }

    let stdout = stdout_buffer
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let stderr = stderr_buffer
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();

    if !status.success() {
        let err_msg = format_volatility_error(plugin, &stdout, &stderr);
        runtime_log(
            LogLevel::Error,
            "volatility",
            format!("Volatility3 basarisiz: {}", err_msg),
        );
        return Err(err_msg);
    }

    let clean_json = trim_to_json(&stdout).ok_or_else(|| {
        let err_msg = format!(
            "Volatility3 JSON çıktısı bulunamadı. Plugin: {plugin}. Çıktı: {}",
            stdout.trim()
        );
        runtime_log(LogLevel::Error, "volatility", &err_msg);
        err_msg
    })?;

    let parsed = serde_json::from_str(clean_json).map_err(|err| {
        let err_msg = format!(
            "Volatility3 JSON çıktısı parse edilemedi: {err}. Plugin: {plugin}. Ham çıktı: {}",
            stdout.trim()
        );
        runtime_log(LogLevel::Error, "volatility", &err_msg);
        err_msg
    })?;
    if let Some(log) = &log {
        log(format!("Volatility3 tamamlandı: {plugin}"));
    }
    runtime_log(
        LogLevel::Info,
        "volatility",
        format!("Volatility3 tamamlandi: {plugin}"),
    );
    Ok(parsed)
}

/// Kullanılabilir sembol dizinlerini ortam değişkenleri ve Volatility klasöründen çıkarır.
fn configured_symbol_dirs(symbol_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = symbol_dir.filter(|path| !path.as_os_str().is_empty()) {
        dirs.push(path.to_path_buf());
    }

    for key in [
        "AMELE_VOLATILITY3_SYMBOL_DIRS",
        "AMELE_VOLATILITY3_SYMBOL_DIR",
    ] {
        if let Ok(value) = env::var(key) {
            for item in value
                .split(';')
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                dirs.push(PathBuf::from(item));
            }
        }
    }

    if let Some(vol_py) = locate_vol_py()
        && let Some(root) = vol_py.parent()
    {
        dirs.push(root.join("volatility3").join("symbols"));
        dirs.push(root.join("symbols"));
    }

    let mut seen = BTreeSet::new();
    dirs.into_iter()
        .filter(|path| path.exists())
        .filter(|path| seen.insert(path.to_string_lossy().to_string()))
        .collect()
}

/// Volatility cache klasörünü kullanıcı cache veya geçici dizin altında hazırlar.
fn volatility_cache_path() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = env::var("XDG_CACHE_HOME") {
        roots.push(PathBuf::from(value));
    }
    if let Ok(home) = env::var("HOME") {
        roots.push(PathBuf::from(home).join(".cache"));
    }
    roots.push(env::temp_dir());

    roots.into_iter().find_map(|root| {
        let path = root.join("amele").join("volatility3");
        runtime_log(
            LogLevel::Debug,
            "volatility",
            format!(
                "Volatility cache dizini kontrol ediliyor: {}",
                path.display()
            ),
        );
        fs::create_dir_all(&path).ok().map(|_| path)
    })
}

/// Birden fazla sembol dizinini Volatility'nin kabul ettiği argüman biçimine çevirir.
fn symbol_dirs_arg(symbol_dir: Option<&Path>) -> Option<String> {
    let dirs = configured_symbol_dirs(symbol_dir);
    if dirs.is_empty() {
        None
    } else {
        Some(
            dirs.iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(";"),
        )
    }
}

/// Alt süreç stdout/stderr satırlarını hem buffer'a hem canlı konsola aktarır.
fn spawn_reader<R>(
    reader: R,
    stream: &'static str,
    buffer: Arc<Mutex<String>>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
    log_all: bool,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(mut output) = buffer.lock() {
                output.push_str(&line);
                output.push('\n');
            }
            let clean = line.trim();
            if clean.is_empty() {
                continue;
            }
            let is_json_payload = clean.starts_with('[') || clean.starts_with('{');
            if (log_all || !is_json_payload)
                && let Some(log) = &log
            {
                log(format!("[volatility:{stream}] {clean}"));
            }
        }
    })
}

/// Volatility çıktısındaki JSON başlangıcını bulup gürültüyü kırpar.
fn trim_to_json(output: &str) -> Option<&str> {
    let array_idx = output.find('[');
    let object_idx = output.find('{');
    let idx = match (array_idx, object_idx) {
        (Some(a), Some(o)) => a.min(o),
        (Some(a), None) => a,
        (None, Some(o)) => o,
        (None, None) => return None,
    };
    Some(output[idx..].trim())
}

/// Volatility hata çıktısını kullanıcıya eylem önerisi içeren mesaja dönüştürür.
fn format_volatility_error(plugin: &str, stdout: &str, stderr: &str) -> String {
    let mut message = format!(
        "Volatility3 hata verdi. Plugin: {plugin}\nStderr: {}\nStdout: {}",
        stderr.trim(),
        stdout.trim()
    );

    let lower = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if lower.contains("invalid choice") {
        message.push_str("\n\nSeçilen Volatility3 kurulumunda bu plugin bulunmuyor. volatility3 klasörünü güncelleyin veya AMELE_VOLATILITY3_PATH ile doğru vol.py yolunu verin.");
    }
    if lower.contains("unsatisfied requirement")
        || lower.contains("layer_name")
        || lower.contains("symbol_table_name")
        || lower.contains("automagic")
    {
        message.push_str(
            "\n\nOlası nedenler:\n\
             1. RAM imajı seçilen işletim sistemi profiliyle uyumlu değil.\n\
             2. Volatility3 sembol tablosu bulunamadı veya indirilemedi.\n\
             3. Dosya fiziksel RAM imajı değil ya da imaj eksik/bozuk.\n\
             4. Linux imajları için kernel banner/sembol eşleşmesi gerekebilir.",
        );
    }
    message
}

/// Seçilen işletim sistemine göre proses listesini döndürür.
pub fn get_processes(file_path: &Path, os_type: &str) -> Result<Vec<VolatilityProcess>, String> {
    get_processes_logged(file_path, os_type, None)
}

/// Volatility banners plugininden kernel banner adaylarını okur.
pub fn get_banners_logged(
    file_path: &Path,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<String>, String> {
    let value = run_volatility_plugin_logged(file_path, "banners.Banners", &[], log.clone())?;
    let rows = json_rows(&value)?;
    let banners = rows
        .iter()
        .filter_map(|item| {
            let offset = value_string(item, &["Offset"]).unwrap_or_else(|| "-".to_string());
            let banner = value_string(item, &["Banner"])?;
            Some(format!("{offset}: {banner}"))
        })
        .collect::<Vec<_>>();

    if let Some(log) = &log {
        if banners.is_empty() {
            log("Linux banner taraması sonuç döndürmedi.".to_string());
        } else {
            log("Bulunan Linux kernel banner adayları:".to_string());
            for banner in banners.iter().take(8) {
                log(format!("banner: {banner}"));
            }
        }
    }

    Ok(banners)
}

/// Volatility başarısız olsa bile ham RAM içinden Linux kernel banner dizgilerini arar.
pub fn scan_linux_banners(
    file_path: &Path,
    limit: usize,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<String>, String> {
    runtime_log(
        LogLevel::Info,
        "volatility",
        format!(
            "Native Linux kernel banner taramasi baslatildi: {}",
            file_path.display()
        ),
    );
    let mut file = File::open(file_path).map_err(|err| {
        let err_msg = format!("RAM imajı açılamadı: {err}");
        runtime_log(LogLevel::Error, "volatility", &err_msg);
        err_msg
    })?;
    let mut buffer = vec![0_u8; LINUX_BANNER_SCAN_CHUNK];
    let mut carry = Vec::new();
    let mut found = BTreeSet::new();
    let mut scanned = 0_u64;

    loop {
        let read = file.read(&mut buffer).map_err(|err| {
            let err_msg = format!("RAM imajı okunamadı: {err}");
            runtime_log(LogLevel::Error, "volatility", &err_msg);
            err_msg
        })?;
        if read == 0 {
            break;
        }
        scanned += read as u64;

        let mut data = Vec::with_capacity(carry.len() + read);
        data.extend_from_slice(&carry);
        data.extend_from_slice(&buffer[..read]);

        let mut cursor = 0;
        while let Some(relative) = find_bytes(&data[cursor..], LINUX_BANNER_NEEDLE) {
            let start = cursor + relative;
            if let Some(banner) = extract_linux_banner(&data[start..])
                && looks_like_kernel_banner(&banner)
            {
                found.insert(banner);
                if found.len() >= limit {
                    let banners = found.into_iter().collect::<Vec<_>>();
                    if let Some(log) = &log {
                        log(format!(
                            "Native Linux banner taraması tamamlandı: {} aday bulundu",
                            banners.len()
                        ));
                    }
                    runtime_log(
                        LogLevel::Info,
                        "volatility",
                        format!(
                            "Linux banner taramasi tamamlandi (limit ulasildi): {} aday",
                            banners.len()
                        ),
                    );
                    return Ok(banners);
                }
            }
            cursor = start + LINUX_BANNER_NEEDLE.len();
        }

        let keep = LINUX_BANNER_MAX_LEN + LINUX_BANNER_NEEDLE.len();
        carry.clear();
        if data.len() > keep {
            carry.extend_from_slice(&data[data.len() - keep..]);
        } else {
            carry.extend_from_slice(&data);
        }
    }

    let banners = found.into_iter().collect::<Vec<_>>();
    if let Some(log) = &log {
        log(format!(
            "Native Linux banner taraması tamamlandı: {} aday, {} MB tarandı",
            banners.len(),
            scanned / 1024 / 1024
        ));
    }
    runtime_log(
        LogLevel::Info,
        "volatility",
        format!(
            "Linux banner taramasi bitti. Toplam {} MB tarandi, {} aday bulundu.",
            scanned / 1024 / 1024,
            banners.len()
        ),
    );
    Ok(banners)
}

/// RAM dosyası analiz edilebilir mi diye Volatility, sembol ve banner ön kontrolü yapar.
pub fn preflight_ram_image(
    file_path: &Path,
    os_type: &str,
    symbol_dir: Option<&Path>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> VolatilityPreflight {
    runtime_log(
        LogLevel::Info,
        "volatility",
        format!(
            "RAM imaji on kontrolu baslatildi. OS: {}, RAM Dosyasi: {}",
            os_type,
            file_path.display()
        ),
    );

    let vol_py = locate_vol_py();
    let mut warnings = Vec::new();
    let mut recommendations = Vec::new();
    let mut banners = Vec::new();
    let mut matching_symbols = Vec::new();
    let mut symbol_count = 0;
    let mut linux_symbol_count = 0;

    if vol_py.is_none() {
        warnings.push("Volatility3 vol.py bulunamadı.".to_string());
        recommendations.push(
            "AMELE_VOLATILITY3_PATH ile vol.py veya volatility3 klasörünü tanımlayın.".to_string(),
        );
        runtime_log(
            LogLevel::Warn,
            "volatility",
            "Volatility3 vol.py bulunamadi.",
        );
    }

    if os_type == "linux" {
        if let Some(log) = &log {
            log("Native Linux kernel banner taraması başlatıldı.".to_string());
        }
        match scan_linux_banners(file_path, 1, log.clone()) {
            Ok(items) => banners = items,
            Err(err) => {
                warnings.push(format!("Linux banner taraması başarısız: {err}"));
                runtime_log(
                    LogLevel::Error,
                    "volatility",
                    format!("Linux banner taramasi hatasi: {err}"),
                );
            }
        }
    }

    if let Some(vol_py_path) = &vol_py {
        match run_isfinfo(vol_py_path, symbol_dir, log.clone()) {
            Ok(rows) => {
                symbol_count = rows.len();
                let banner_needles = banners
                    .iter()
                    .map(|banner| banner.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                for row in rows {
                    let raw = serde_json::to_string(&row).unwrap_or_default();
                    let lower = raw.to_ascii_lowercase();
                    if lower.contains("linux") {
                        linux_symbol_count += 1;
                    }
                    for banner in &banner_needles {
                        if lower.contains(banner) {
                            matching_symbols.push(raw.clone());
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                warnings.push(format!("Volatility ISF bilgisi alınamadı: {err}"));
                runtime_log(
                    LogLevel::Error,
                    "volatility",
                    format!("ISF bilgisi alma hatasi: {err}"),
                );
            }
        }
    }

    if os_type == "linux" {
        if banners.is_empty() {
            warnings.push("RAM imajında Linux kernel banner adayı bulunamadı.".to_string());
            recommendations.push(
                "Dosyanın ham fiziksel RAM imajı olduğundan ve edinimin temiz tamamlandığından emin olun."
                    .to_string(),
            );
            runtime_log(
                LogLevel::Warn,
                "volatility",
                "Linux kernel banner adayi bulunamadi.",
            );
        } else if matching_symbols.is_empty() {
            warnings.push(format!(
                "Linux kernel banner bulundu ama eşleşen Volatility3 ISF symbol dosyası yok: {}",
                banners[0]
            ));
            recommendations.push(
                "Bu kernel için dwarf2json ile ISF üretip volatility3/symbols/linux altına koyun."
                    .to_string(),
            );
            runtime_log(
                LogLevel::Warn,
                "volatility",
                "Eşleşen Volatility3 ISF symbol dosyası yok.",
            );
        }
    }

    let symbol_dirs = configured_symbol_dirs(symbol_dir)
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if symbol_dirs.is_empty() {
        warnings.push("Volatility symbol dizini bulunamadı.".to_string());
        runtime_log(
            LogLevel::Warn,
            "volatility",
            "Volatility symbol dizini bulunamadı.",
        );
    }

    let ready = if os_type == "linux" {
        vol_py.is_some() && !banners.is_empty() && !matching_symbols.is_empty()
    } else {
        vol_py.is_some()
    };

    if ready {
        runtime_log(
            LogLevel::Info,
            "volatility",
            "RAM imaji on kontrolu basarili. Volatility hazır.",
        );
    } else {
        runtime_log(
            LogLevel::Warn,
            "volatility",
            "RAM imaji on kontrolu tamamlanamadi veya eksiklikler var.",
        );
    }

    VolatilityPreflight {
        ready,
        vol_py: vol_py.map(|path| path.display().to_string()),
        symbol_dirs,
        symbol_count,
        linux_symbol_count,
        banners,
        matching_symbols,
        warnings,
        recommendations,
    }
}

/// Volatility isfinfo çıktısını alarak mevcut sembol tablolarını listeler.
fn run_isfinfo(
    vol_py: &Path,
    symbol_dir: Option<&Path>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<Value>, String> {
    let vol_dir = vol_py.parent().unwrap_or(Path::new("."));
    let mut args = vec![vol_py.to_string_lossy().into_owned()];
    if let Some(symbols_arg) = symbol_dirs_arg(symbol_dir) {
        args.push("-s".to_string());
        args.push(symbols_arg);
    }
    if let Some(cache_path) = volatility_cache_path() {
        args.push("--cache-path".to_string());
        args.push(cache_path.to_string_lossy().into_owned());
    }
    args.extend([
        "-q".to_string(),
        "-r".to_string(),
        "json".to_string(),
        "isfinfo.IsfInfo".to_string(),
    ]);
    if let Some(log) = &log {
        log("Volatility ISF/symbol bilgisi kontrol ediliyor.".to_string());
    }
    let value = run_volatility_command(vol_dir, args, "isfinfo.IsfInfo", log)?;
    json_rows(&value)
}

/// Byte dizisi içinde küçük imza araması yapar.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Ham bellekten Linux kernel banner metnini güvenli uzunlukta çıkarır.
fn extract_linux_banner(data: &[u8]) -> Option<String> {
    let end = data
        .iter()
        .take(LINUX_BANNER_MAX_LEN)
        .position(|byte| matches!(*byte, 0 | b'\n' | b'\r'))
        .unwrap_or_else(|| data.len().min(LINUX_BANNER_MAX_LEN));
    let raw = &data[..end];
    let text = String::from_utf8_lossy(raw)
        .chars()
        .take_while(|ch| ch.is_ascii_graphic() || *ch == ' ')
        .collect::<String>();
    let clean = text.trim().to_string();
    if clean.is_empty() { None } else { Some(clean) }
}

/// Bulunan dizginin gerçek kernel bannerına benzeyip benzemediğini kontrol eder.
fn looks_like_kernel_banner(text: &str) -> bool {
    text.starts_with("Linux version ")
        && !text.contains("%s")
        && !text.contains("http")
        && (text.contains(" SMP ")
            || text.contains("PREEMPT")
            || text.contains("GNU ld")
            || text.contains("gcc")
            || text.contains("#1"))
}

/// Proses listesini canlı log desteğiyle alır.
pub fn get_processes_logged(
    file_path: &Path,
    os_type: &str,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<VolatilityProcess>, String> {
    get_processes_logged_with_symbol_dir(file_path, os_type, None, log)
}

/// OS türüne göre doğru Volatility pslist pluginini seçer ve satırları ortak modele çevirir.
pub fn get_processes_logged_with_symbol_dir(
    file_path: &Path,
    os_type: &str,
    symbol_dir: Option<&Path>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<VolatilityProcess>, String> {
    let plugin = match os_type {
        "windows" => "windows.pslist.PsList",
        "linux" => "linux.pslist.PsList",
        _ => return Err(format!("Desteklenmeyen RAM işletim sistemi: {os_type}")),
    };

    let value =
        run_volatility_plugin_logged_with_symbol_dir(file_path, plugin, &[], symbol_dir, log)?;
    let rows = json_rows(&value)?;

    Ok(rows
        .iter()
        .map(|item| {
            if os_type == "linux" {
                linux_process(item)
            } else {
                windows_process(item)
            }
        })
        .filter(|process| process.pid > 0 || process.name != "Unknown")
        .collect())
}

/// Windows pslist satırını ortak VolatilityProcess modeline dönüştürür.
fn windows_process(item: &Value) -> VolatilityProcess {
    let pid = value_i64(item, &["PID"]).unwrap_or(0);
    let ppid = value_i64(item, &["PPID"]).unwrap_or(0);
    let name = value_string(item, &["ImageFileName", "Image File Name", "Process"])
        .unwrap_or_else(|| "Unknown".to_string());
    let offset = value_string(
        item,
        &["Offset(V)", "Offset (V)", "Offset(P)", "Offset (P)"],
    )
    .unwrap_or_else(|| "N/A".to_string());
    let threads = value_string(item, &["Threads"]).unwrap_or_else(|| "0".to_string());
    let handles = value_string(item, &["Handles"]).unwrap_or_else(|| "0".to_string());
    let session = value_string(item, &["SessionId"]).unwrap_or_else(|| "-".to_string());
    let create_time = value_string(item, &["CreateTime"]).unwrap_or_else(|| "-".to_string());

    VolatilityProcess {
        pid,
        ppid,
        name,
        offset,
        extra_info: format!(
            "Threads: {threads} · Handles: {handles} · Session: {session} · Created: {create_time}"
        ),
    }
}

/// Linux pslist satırını ortak VolatilityProcess modeline dönüştürür.
fn linux_process(item: &Value) -> VolatilityProcess {
    let pid = value_i64(item, &["PID"]).unwrap_or(0);
    let ppid = value_i64(item, &["PPID"]).unwrap_or(0);
    let name = value_string(item, &["COMM", "Command", "Process"])
        .unwrap_or_else(|| "Unknown".to_string());
    let offset = value_string(item, &["OFFSET (V)", "Offset(V)", "Offset (V)"])
        .unwrap_or_else(|| "N/A".to_string());
    let uid = value_string(item, &["UID"]).unwrap_or_else(|| "-".to_string());
    let gid = value_string(item, &["GID"]).unwrap_or_else(|| "-".to_string());
    let euid = value_string(item, &["EUID"]).unwrap_or_else(|| "-".to_string());
    let egid = value_string(item, &["EGID"]).unwrap_or_else(|| "-".to_string());
    let create_time =
        value_string(item, &["CREATION TIME", "CreateTime"]).unwrap_or_else(|| "-".to_string());

    VolatilityProcess {
        pid,
        ppid,
        name,
        offset,
        extra_info: format!(
            "UID/GID: {uid}/{gid} · EUID/EGID: {euid}/{egid} · Created: {create_time}"
        ),
    }
}

/// Seçilen prosesin ayrıntılarını logsuz olarak üretir.
pub fn get_process_details(file_path: &Path, os_type: &str, pid: i64) -> Result<String, String> {
    get_process_details_logged(file_path, os_type, pid, None)
}

/// Seçilen proses ayrıntılarını canlı log desteğiyle üretir.
pub fn get_process_details_logged(
    file_path: &Path,
    os_type: &str,
    pid: i64,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<String, String> {
    get_process_details_logged_with_symbol_dir(file_path, os_type, pid, None, log)
}

/// OS türüne göre DLL listesi veya açık dosya listesini Volatility ile çıkarır.
pub fn get_process_details_logged_with_symbol_dir(
    file_path: &Path,
    os_type: &str,
    pid: i64,
    symbol_dir: Option<&Path>,
    log: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<String, String> {
    let pid_str = pid.to_string();
    let (plugin, extra_args) = match os_type {
        "windows" => ("windows.dlllist.DllList", vec!["--pid", pid_str.as_str()]),
        "linux" => ("linux.lsof.Lsof", vec!["--pid", pid_str.as_str()]),
        _ => return Err(format!("Desteklenmeyen RAM işletim sistemi: {os_type}")),
    };

    let value = run_volatility_plugin_logged_with_symbol_dir(
        file_path,
        plugin,
        &extra_args,
        symbol_dir,
        log,
    )?;
    let rows = json_rows(&value)?;

    if os_type == "linux" {
        Ok(format_linux_open_files(&rows))
    } else {
        Ok(format_windows_dlls(&rows))
    }
}

/// Windows DLL satırlarını okunabilir metin tablosuna çevirir.
fn format_windows_dlls(rows: &[Value]) -> String {
    let mut details = format!(
        "LOADED DLL LIST\n{:<8} {:<22} {:<14} {:<10} {}\n",
        "PID", "Base", "Size", "LoadCount", "Path"
    );
    details.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for item in rows {
        let pid = value_string(item, &["PID"]).unwrap_or_else(|| "-".to_string());
        let base = value_string(item, &["Base"]).unwrap_or_else(|| "-".to_string());
        let size = value_string(item, &["Size"]).unwrap_or_else(|| "-".to_string());
        let load_count = value_string(item, &["LoadCount"]).unwrap_or_else(|| "-".to_string());
        let path = value_string(item, &["Path", "Name"]).unwrap_or_else(|| "-".to_string());

        details.push_str(&format!(
            "{:<8} {:<22} {:<14} {:<10} {}\n",
            pid, base, size, load_count, path
        ));
    }

    if rows.is_empty() {
        details.push_str("No DLL rows returned.\n");
    }
    details
}

/// Linux açık dosya satırlarını okunabilir metin tablosuna çevirir.
fn format_linux_open_files(rows: &[Value]) -> String {
    let mut details = format!(
        "OPEN FILES\n{:<8} {:<8} {:<20} {:<8} {:<10} {}\n",
        "PID", "FD", "Process", "Type", "Size", "Path"
    );
    details.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for item in rows {
        let pid = value_string(item, &["PID"]).unwrap_or_else(|| "-".to_string());
        let fd = value_string(item, &["FD"]).unwrap_or_else(|| "-".to_string());
        let process = value_string(item, &["Process", "COMM"]).unwrap_or_else(|| "-".to_string());
        let inode_type = value_string(item, &["Type"]).unwrap_or_else(|| "-".to_string());
        let size = value_string(item, &["Size"]).unwrap_or_else(|| "-".to_string());
        let path = value_string(item, &["Path"]).unwrap_or_else(|| "-".to_string());

        details.push_str(&format!(
            "{:<8} {:<8} {:<20} {:<8} {:<10} {}\n",
            pid, fd, process, inode_type, size, path
        ));
    }

    if rows.is_empty() {
        details.push_str("No open file rows returned.\n");
    }
    details
}

/// Volatility JSON çıktısındaki satır dizisini farklı formatlardan normalize eder.
fn json_rows(value: &Value) -> Result<Vec<Value>, String> {
    if let Some(arr) = value.as_array() {
        return Ok(arr.clone());
    }
    for key in ["rows", "data", "tree"] {
        if let Some(arr) = value.get(key).and_then(Value::as_array) {
            return Ok(arr.clone());
        }
    }
    Err("Volatility3 JSON çıktısı satır listesi içermiyor.".to_string())
}

/// JSON alanını i64 değerine güvenli şekilde çevirir.
fn value_i64(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        let value = item.get(*key)?;
        match value {
            Value::Number(number) => number
                .as_i64()
                .or_else(|| number.as_u64().map(|n| n as i64)),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        }
    })
}

/// JSON alanını gösterilebilir string değerine güvenli şekilde çevirir.
fn value_string(item: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = item.get(*key)?;
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => number
                .as_u64()
                .map(|n| {
                    if key.to_ascii_lowercase().contains("offset")
                        || matches!(*key, "Base" | "Size")
                    {
                        format!("0x{n:X}")
                    } else {
                        n.to_string()
                    }
                })
                .or_else(|| number.as_i64().map(|n| n.to_string())),
            Value::Bool(value) => Some(value.to_string()),
            Value::Null => None,
            other => Some(other.to_string()),
        }
    })
}
