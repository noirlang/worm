//! AVML/WinPMEM durum kontrolü, RAM edinimi ve süreç kontrolünü yönetir.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::logging::{LogLevel, runtime_log};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const CONTROL_RUNNING: u8 = 0;
const CONTROL_PAUSED: u8 = 1;
const CONTROL_CANCELLED: u8 = 2;
pub const WINPMEM_NAME: &str = "go-winpmem_amd64_1.0-rc2_signed.exe";

/// RAM araçlarının bulunma, yetki ve fiziksel RAM durumunu UI'ye taşır.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RamToolStatus {
    pub tool_present: bool,
    pub admin_privilege: bool,
    pub ram_size: u64,
    pub tool_path: Option<PathBuf>,
    pub message: String,
}

/// RAM edinimi tamamlandığında çıktı dosyası ve yazılan bayt bilgisini taşır.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RamAcquisitionResult {
    pub output_file: PathBuf,
    pub bytes_written: u64,
}

/// RAM edinimi için duraklatma, devam ve iptal kontrolünü thread-safe taşır.
#[derive(Clone)]
pub struct CancellationToken {
    state: Arc<AtomicU8>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(CONTROL_RUNNING)),
        }
    }
}

impl CancellationToken {
    /// Edinim sürecine iptal sinyali gönderir.
    pub fn cancel(&self) {
        self.state.store(CONTROL_CANCELLED, Ordering::SeqCst);
    }

    /// Edinim sürecini duraklatma durumuna alır.
    pub fn pause(&self) {
        self.state.store(CONTROL_PAUSED, Ordering::SeqCst);
    }

    /// Duraklatılmış edinim sürecini devam ettirir.
    pub fn resume(&self) {
        self.state.store(CONTROL_RUNNING, Ordering::SeqCst);
    }

    /// İptal sinyali gönderilip gönderilmediğini kontrol eder.
    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) == CONTROL_CANCELLED
    }

    /// Duraklatma sinyali aktif mi diye kontrol eder.
    pub fn is_paused(&self) -> bool {
        self.state.load(Ordering::SeqCst) == CONTROL_PAUSED
    }
}

/// Linux tarafında AVML aracının kurulu ve kullanılabilir olup olmadığını kontrol eder.
pub fn avml_status(candidate: Option<&Path>) -> RamToolStatus {
    let tool = find_avml(candidate);
    RamToolStatus {
        tool_present: tool.is_some(),
        admin_privilege: is_root_or_admin(),
        ram_size: physical_ram_size(),
        tool_path: tool.clone(),
        message: if tool.is_some() {
            "AVML ready".to_string()
        } else {
            "AVML not found".to_string()
        },
    }
}

/// AVML ile Linux RAM imajı alır ve çıktı dosyasının büyümesine göre ilerleme döndürür.
pub fn acquire_with_avml<F>(
    output_file: impl AsRef<Path>,
    candidate: Option<&Path>,
    cancellation: &CancellationToken,
    progress: F,
) -> AmeleResult<RamAcquisitionResult>
where
    F: FnMut(u64, u64),
{
    #[cfg(windows)]
    {
        let _ = output_file;
        let _ = candidate;
        let _ = cancellation;
        let _ = progress;
        Err(AmeleError::new(
            HataKodu::YetkisizErisim,
            "AVML Windows uzerinde kullanilmaz",
        ))
    }

    #[cfg(not(windows))]
    {
        let mut progress = progress;
        runtime_log(
            LogLevel::Info,
            "ram",
            format!(
                "AVML ile RAM edinimi baslatildi. Cikis dosyasi: {}",
                output_file.as_ref().display()
            ),
        );
        if !is_root_or_admin() {
            let err = AmeleError::new(
                HataKodu::YetkisizErisim,
                "RAM edinimi icin root yetkisi gerekli",
            );
            runtime_log(LogLevel::Error, "ram", format!("Yetki hatasi: {:?}", err));
            return Err(err);
        }

        let avml = find_avml(candidate).ok_or_else(|| {
            let err = AmeleError::new(HataKodu::DosyaAcilamadi, "AVML bulunamadi");
            runtime_log(
                LogLevel::Error,
                "ram",
                format!("AVML bulunamadi hatasi: {:?}", err),
            );
            err
        })?;
        if let Some(parent) = output_file.as_ref().parent() {
            runtime_log(
                LogLevel::Info,
                "ram",
                format!("RAM cikti klasoru olusturuluyor: {}", parent.display()),
            );
            fs::create_dir_all(parent).map_err(|err| {
                let w_err = AmeleError::io(
                    HataKodu::DosyaYazma,
                    "RAM cikti klasoru olusturulamadi",
                    err,
                );
                runtime_log(
                    LogLevel::Error,
                    "ram",
                    format!("Klasor olusturma hatasi: {:?}", w_err),
                );
                w_err
            })?;
        }

        let total = physical_ram_size();

        // AVML surumunun alt komut (acquire) isteyip istemedigini --help ile kontrol et
        let use_acquire = match Command::new(&avml).arg("--help").output() {
            Ok(output) if output.status.success() => {
                let help_text = String::from_utf8_lossy(&output.stdout);
                help_text.contains("acquire")
            }
            Ok(output) => {
                let help_text = String::from_utf8_lossy(&output.stderr);
                help_text.contains("acquire")
            }
            Err(_) => false,
        };

        runtime_log(
            LogLevel::Info,
            "ram",
            format!(
                "AVML komutu calistiriliyor: {} (acquire subcommand: {}) {}",
                avml.display(),
                use_acquire,
                output_file.as_ref().display()
            ),
        );

        let mut cmd = Command::new(&avml);
        if use_acquire {
            cmd.arg("acquire");
        }
        cmd.arg(output_file.as_ref());

        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                let w_err = AmeleError::io(HataKodu::Genel, "AVML baslatilamadi", err);
                runtime_log(
                    LogLevel::Error,
                    "ram",
                    format!("AVML spawn hatasi: {:?}", w_err),
                );
                w_err
            })?;
        monitor_child_file(
            &mut child,
            output_file.as_ref(),
            total,
            Duration::from_secs(7200),
            cancellation,
            &mut progress,
        )
    }
}

/// Windows tarafında WinPMEM aracının kurulu ve kullanılabilir olup olmadığını kontrol eder.
pub fn winpmem_status(candidate: Option<&Path>) -> RamToolStatus {
    let tool = find_winpmem(candidate);
    RamToolStatus {
        tool_present: tool.is_some(),
        admin_privilege: is_root_or_admin(),
        ram_size: physical_ram_size(),
        tool_path: tool.clone(),
        message: if tool.is_some() {
            "WinPMEM ready".to_string()
        } else {
            "WinPMEM not found".to_string()
        },
    }
}

/// WinPMEM ile Windows RAM imajı alır ve çıktı dosyasının büyümesine göre ilerleme döndürür.
pub fn acquire_with_winpmem<F>(
    output_file: impl AsRef<Path>,
    candidate: Option<&Path>,
    cancellation: &CancellationToken,
    progress: F,
) -> AmeleResult<RamAcquisitionResult>
where
    F: FnMut(u64, u64),
{
    #[cfg(not(windows))]
    {
        let _ = output_file;
        let _ = candidate;
        let _ = cancellation;
        let _ = progress;
        Err(AmeleError::new(
            HataKodu::YetkisizErisim,
            "WinPMEM sadece Windows uzerinde desteklenir",
        ))
    }

    #[cfg(windows)]
    {
        let mut progress = progress;
        runtime_log(
            LogLevel::Info,
            "ram",
            format!(
                "WinPMEM ile RAM edinimi baslatildi. Cikis dosyasi: {}",
                output_file.as_ref().display()
            ),
        );
        if !is_root_or_admin() {
            let err = AmeleError::new(
                HataKodu::YetkisizErisim,
                "Administrator privileges required",
            );
            runtime_log(LogLevel::Error, "ram", format!("Yetki hatasi: {:?}", err));
            return Err(err);
        }
        let winpmem = find_winpmem(candidate).ok_or_else(|| {
            let err = AmeleError::new(HataKodu::DosyaAcilamadi, "WinPMEM bulunamadi");
            runtime_log(
                LogLevel::Error,
                "ram",
                format!("WinPMEM bulunamadi hatasi: {:?}", err),
            );
            err
        })?;
        if let Some(parent) = output_file.as_ref().parent() {
            runtime_log(
                LogLevel::Info,
                "ram",
                format!("RAM cikti klasoru olusturuluyor: {}", parent.display()),
            );
            fs::create_dir_all(parent).map_err(|err| {
                let w_err = AmeleError::io(
                    HataKodu::DosyaYazma,
                    "RAM cikti klasoru olusturulamadi",
                    err,
                );
                runtime_log(
                    LogLevel::Error,
                    "ram",
                    format!("Klasor olusturma hatasi: {:?}", w_err),
                );
                w_err
            })?;
        }

        let total = physical_ram_size();
        let commands: Vec<Vec<std::ffi::OsString>> = vec![
            vec![
                winpmem.clone().into_os_string(),
                "acquire".into(),
                output_file.as_ref().as_os_str().to_os_string(),
            ],
            vec![
                winpmem.clone().into_os_string(),
                "acquire".into(),
                "--output".into(),
                output_file.as_ref().as_os_str().to_os_string(),
            ],
            vec![
                winpmem.clone().into_os_string(),
                "-o".into(),
                output_file.as_ref().as_os_str().to_os_string(),
                "-1".into(),
            ],
        ];

        let mut last_error = String::new();
        for command in commands {
            let mut iter = command.into_iter();
            let executable = iter.next().expect("command has executable");
            let args_debug: Vec<String> = iter
                .clone()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            runtime_log(
                LogLevel::Info,
                "ram",
                format!(
                    "WinPMEM komutu calistiriliyor: {} {:?}",
                    executable.to_string_lossy(),
                    args_debug
                ),
            );
            let mut child = Command::new(executable)
                .args(iter)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            match child.as_mut() {
                Ok(child) => {
                    let result = monitor_child_file(
                        child,
                        output_file.as_ref(),
                        total,
                        Duration::from_secs(3600),
                        cancellation,
                        &mut progress,
                    );
                    if result.is_ok() {
                        return result;
                    }
                    last_error = result.err().map(|err| err.to_string()).unwrap_or_default();
                    runtime_log(
                        LogLevel::Warn,
                        "ram",
                        format!(
                            "WinPMEM komutu basarisiz oldu, sonraki denenecek. Hata: {}",
                            last_error
                        ),
                    );
                }
                Err(err) => {
                    last_error = err.to_string();
                    runtime_log(
                        LogLevel::Warn,
                        "ram",
                        format!("WinPMEM spawn hatasi: {}", last_error),
                    );
                }
            }
        }

        let w_err = AmeleError::new(
            HataKodu::Genel,
            format!("WinPMEM komutu baslatilamadi: {last_error}"),
        );
        runtime_log(
            LogLevel::Error,
            "ram",
            format!("WinPMEM tamamen basarisiz: {:?}", w_err),
        );
        Err(w_err)
    }
}

/// Harici RAM aracını izler; pause/resume/stop ve timeout davranışını uygular.
fn monitor_child_file<F>(
    child: &mut Child,
    output_file: &Path,
    total: u64,
    timeout: Duration,
    cancellation: &CancellationToken,
    progress: &mut F,
) -> AmeleResult<RamAcquisitionResult>
where
    F: FnMut(u64, u64),
{
    let started = Instant::now();
    let mut child_paused = false;
    loop {
        if cancellation.is_cancelled() {
            runtime_log(
                LogLevel::Warn,
                "ram",
                "RAM edinimi iptal edildi. Alt surec sonlandiriliyor.",
            );
            if child_paused {
                resume_child(child);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(AmeleError::new(HataKodu::Genel, "RAM edinimi iptal edildi"));
        }

        if cancellation.is_paused() {
            if !child_paused {
                runtime_log(LogLevel::Info, "ram", "RAM edinimi duraklatiliyor...");
                pause_child(child);
                child_paused = true;
            }
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        if child_paused {
            runtime_log(LogLevel::Info, "ram", "RAM edinimi devam ettiriliyor...");
            resume_child(child);
            child_paused = false;
        }

        if let Ok(Some(status)) = child.try_wait() {
            let size = fs::metadata(output_file)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if status.success() && size > 0 {
                runtime_log(
                    LogLevel::Info,
                    "ram",
                    format!("RAM edinimi tamamlandi. Toplam {} bayt yazildi.", size),
                );
                progress(size, total);
                return Ok(RamAcquisitionResult {
                    output_file: output_file.to_path_buf(),
                    bytes_written: size,
                });
            }
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            let stderr = stderr.trim();
            let detail = if stderr.is_empty() {
                format!("RAM araci basarisiz oldu: {status}")
            } else {
                format!("RAM araci basarisiz oldu: {status}\nAyrıntı: {stderr}")
            };
            runtime_log(
                LogLevel::Error,
                "ram",
                format!("RAM araci hatasi: {}", detail),
            );
            return Err(AmeleError::new(HataKodu::DosyaYazma, detail));
        }

        if started.elapsed() > timeout {
            runtime_log(
                LogLevel::Error,
                "ram",
                "RAM edinimi zaman asimi limitine ulasti.",
            );
            let _ = child.kill();
            let _ = child.wait();
            return Err(AmeleError::new(HataKodu::Genel, "RAM edinimi zaman asimi"));
        }

        if let Ok(metadata) = fs::metadata(output_file) {
            progress(metadata.len(), total);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

/// Harici RAM aracını işletim sistemi sinyaliyle duraklatır.
fn pause_child(child: &Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGSTOP);
    }

    #[cfg(windows)]
    {
        runtime_log(
            LogLevel::Info,
            "ram",
            format!(
                "Windows alt sureci duraklatmak icin powershell cagiriliyor, PID={}",
                child.id()
            ),
        );
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Suspend-Process -Id {} -ErrorAction SilentlyContinue",
                    child.id()
                ),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Duraklatılmış harici RAM aracını devam ettirir.
fn resume_child(child: &Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGCONT);
    }

    #[cfg(windows)]
    {
        runtime_log(
            LogLevel::Info,
            "ram",
            format!(
                "Windows alt sureci devam ettirmek icin powershell cagiriliyor, PID={}",
                child.id()
            ),
        );
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Resume-Process -Id {} -ErrorAction SilentlyContinue",
                    child.id()
                ),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Linux'ta PATH veya bilinen konumlarda AVML binary'sini arar.
pub fn find_avml(candidate: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = candidate
        && path.exists()
    {
        return Some(path.to_path_buf());
    }

    find_in_path("avml").or_else(|| {
        ["/usr/bin/avml", "/usr/local/bin/avml"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
    })
}

/// Windows'ta PATH veya bilinen konumlarda WinPMEM binary'sini arar.
pub fn find_winpmem(candidate: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = candidate
        && path.exists()
    {
        return Some(path.to_path_buf());
    }

    find_in_path(WINPMEM_NAME).or_else(|| {
        let mut candidates = vec![
            PathBuf::from(r"C:\Tools\go-winpmem_amd64_1.0-rc2_signed.exe"),
            PathBuf::from(r"C:\Forensics\go-winpmem_amd64_1.0-rc2_signed.exe"),
        ];
        // Çalışan uygulamanın yanındaki WinPMEM kopyası da kontrol edilir.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                candidates.insert(0, exe_dir.join(WINPMEM_NAME));
            }
        }
        candidates.into_iter().find(|path| path.exists())
    })
}

/// PATH içindeki binary adayını bulur.
fn find_in_path(binary: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(binary))
        .find(|path| path.exists())
}

/// Sistemdeki fiziksel RAM miktarını platforma göre hesaplar.
pub fn physical_ram_size() -> u64 {
    #[cfg(target_os = "linux")]
    {
        runtime_log(
            LogLevel::Debug,
            "ram",
            "Linux fiziksel RAM boyutu /proc/meminfo dosyasindan okunuyor.",
        );
        if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    let kb = rest
                        .split_whitespace()
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    let bytes = kb * 1024;
                    runtime_log(
                        LogLevel::Info,
                        "ram",
                        format!("Fiziksel RAM boyutu saptandi: {} bayt", bytes),
                    );
                    return bytes;
                }
            }
        } else {
            runtime_log(
                LogLevel::Warn,
                "ram",
                "/proc/meminfo okunurken hata olustu.",
            );
        }
        0
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        runtime_log(
            LogLevel::Debug,
            "ram",
            "Windows fiziksel RAM boyutu GlobalMemoryStatusEx ile aliniyor.",
        );
        let mut info = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };
        let ok = unsafe { GlobalMemoryStatusEx(&mut info) };
        if ok != 0 {
            runtime_log(
                LogLevel::Info,
                "ram",
                format!("Fiziksel RAM boyutu saptandi: {} bayt", info.ullTotalPhys),
            );
            info.ullTotalPhys
        } else {
            runtime_log(
                LogLevel::Warn,
                "ram",
                "GlobalMemoryStatusEx basarisiz oldu.",
            );
            0
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        0
    }
}

/// Sürecin root/admin yetkisiyle çalışıp çalışmadığını kontrol eder.
pub fn is_root_or_admin() -> bool {
    #[cfg(windows)]
    {
        runtime_log(LogLevel::Debug, "ram", "Windows yetkileri denetleniyor...");
        Command::new("cmd")
            .args(["/C", "net", "session"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(unix)]
    {
        runtime_log(
            LogLevel::Debug,
            "ram",
            "Unix/Linux root yetkisi geteuid ile denetleniyor...",
        );
        unsafe { libc::geteuid() == 0 }
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn linux_ram_size_is_nonzero_on_linux() {
        #[cfg(target_os = "linux")]
        assert!(physical_ram_size() > 0);
    }
}
