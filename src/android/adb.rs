//! ADB kurulumunu, bağlı cihaz listesini ve temel ADB komutlarını yönetir.
use serde::Serialize;
use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
/// ADB binary durumunu, sürümünü ve yolunu arayüze taşır.
pub struct AdbStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
/// ADB kurulum komutunun sonucunu ve kurulum sonrası durumu taşır.
pub struct AdbInstallResult {
    pub installed: bool,
    pub package_manager: String,
    pub command: String,
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub message: String,
    pub status: AdbStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// adb devices -l çıktısından ayrıştırılan tek Android cihazı temsil eder.
pub struct AndroidDevice {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
    pub raw: String,
}

/// Sistemde ADB kurulu mu ve çalışıyor mu diye kontrol eder.
pub fn adb_status() -> AdbStatus {
    #[cfg(windows)]
    refresh_windows_path();

    crate::logging::runtime_log(
        crate::logging::LogLevel::Debug,
        "android:adb",
        "ADB durumu kontrol ediliyor...",
    );
    match Command::new("adb").arg("version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = parse_adb_version(&stdout);
            let path = parse_adb_path(&stdout).or_else(|| command_path("adb"));
            let detail = version
                .clone()
                .or_else(|| path.clone())
                .unwrap_or_else(|| "adb".to_string());

            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "android:adb",
                format!("ADB calisiyor: {}", detail),
            );

            AdbStatus {
                installed: true,
                version,
                path,
                message: format!("ADB bulundu: {}", detail),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let message = first_non_empty(&stderr)
                .or_else(|| first_non_empty(&stdout))
                .unwrap_or_else(|| "ADB calistirildi ama surum alinamadi".to_string());

            crate::logging::runtime_log(
                crate::logging::LogLevel::Warn,
                "android:adb",
                format!("ADB calisti ama basarisiz oldu: {}", message),
            );

            AdbStatus {
                installed: false,
                version: None,
                path: command_path("adb"),
                message,
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Warn,
                "android:adb",
                "ADB bulunamadi (hata: NotFound)",
            );
            AdbStatus {
                installed: false,
                version: None,
                path: None,
                message: "ADB bulunamadi".to_string(),
            }
        }
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:adb",
                format!("ADB kontrol edilirken beklenmeyen hata: {}", err),
            );
            AdbStatus {
                installed: false,
                version: None,
                path: None,
                message: format!("ADB kontrol edilemedi: {err}"),
            }
        }
    }
}

/// Platform paket yöneticisini kullanarak ADB kurulumu yapar.
pub fn install_adb() -> Result<AdbInstallResult, String> {
    let before = adb_status();
    if before.installed {
        return Ok(AdbInstallResult {
            installed: true,
            package_manager: "already-installed".to_string(),
            command: "adb version".to_string(),
            status_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            message: before.message.clone(),
            status: before,
        });
    }

    let command = adb_install_command()?;
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:adb:install",
        format!("ADB kurulum komutu baslatiliyor: {}", command.display()),
    );

    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|err| format!("ADB kurulum komutu başlatılamadı: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:adb:install",
            format!(
                "ADB kurulum komutu basarisiz: {} | stdout={} | stderr={}",
                command.display(),
                stdout,
                stderr
            ),
        );
        return Err(format!(
            "ADB kurulamadı.\nKomut: {}\nÇıkış kodu: {:?}\nstdout: {}\nstderr: {}",
            command.display(),
            output.status.code(),
            if stdout.is_empty() { "-" } else { &stdout },
            if stderr.is_empty() { "-" } else { &stderr },
        ));
    }

    let status = adb_status();
    if !status.installed {
        return Err(format!(
            "Paket yöneticisi başarı döndürdü ama ADB hâlâ PATH içinde görünmüyor.\nKomut: {}\nstdout: {}\nstderr: {}\nÇözüm: Uygulamayı yeniden başlatın veya adb binary yolunu PATH içine ekleyin.",
            command.display(),
            if stdout.is_empty() { "-" } else { &stdout },
            if stderr.is_empty() { "-" } else { &stderr },
        ));
    }

    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:adb:install",
        format!("ADB kuruldu: {}", status.message),
    );
    let command_display = command.display();

    Ok(AdbInstallResult {
        installed: true,
        package_manager: command.manager,
        command: command_display,
        status_code: output.status.code(),
        stdout,
        stderr,
        message: status.message.clone(),
        status,
    })
}

/// Bağlı Android cihazlarını adb devices -l ile listeler.
pub fn list_devices() -> Result<Vec<AndroidDevice>, String> {
    crate::logging::runtime_log(
        crate::logging::LogLevel::Debug,
        "android:adb",
        "Bagli Android cihazlari listeleniyor...",
    );
    let mut output = run_adb_devices_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = first_non_empty(&stderr)
            .or_else(|| first_non_empty(&stdout))
            .unwrap_or_else(|| "ADB cihaz listesi basarisiz oldu".to_string());

        if detail.contains("daemon not running") || detail.contains("daemon started") {
            std::thread::sleep(Duration::from_millis(500));
            output = run_adb_devices_output()?;
        }
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = first_non_empty(&stderr)
            .or_else(|| first_non_empty(&stdout))
            .unwrap_or_else(|| "ADB cihaz listesi basarisiz oldu".to_string());

        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:adb",
            format!("ADB devices komutu basarisiz cikis kodu: {}", detail),
        );
        return Err(detail);
    }

    let devices_raw = String::from_utf8_lossy(&output.stdout);
    let devices = parse_adb_devices(&devices_raw);

    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:adb",
        format!(
            "Bulunan Android cihaz sayisi: {} ({:?})",
            devices.len(),
            devices.iter().map(|d| &d.serial).collect::<Vec<_>>()
        ),
    );
    Ok(devices)
}

#[derive(Debug, Clone)]
struct InstallCommand {
    manager: String,
    program: String,
    args: Vec<String>,
}

impl InstallCommand {
    fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(shell_display_arg)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn adb_install_command() -> Result<InstallCommand, String> {
    #[cfg(windows)]
    {
        if command_path("winget").is_none() {
            return Err(
                "winget bulunamadı. Windows App Installer kurulu olmalı veya Microsoft Store üzerinden güncellenmeli."
                    .to_string(),
            );
        }
        return Ok(InstallCommand {
            manager: "winget".to_string(),
            program: "winget".to_string(),
            args: vec![
                "install".to_string(),
                "--id".to_string(),
                "Google.PlatformTools".to_string(),
                "--exact".to_string(),
                "--accept-source-agreements".to_string(),
                "--accept-package-agreements".to_string(),
            ],
        });
    }

    #[cfg(target_os = "linux")]
    {
        let base = linux_adb_install_base_command()?;
        return Ok(elevate_linux_install_command(base));
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err("ADB otomatik kurulumu bu platformda desteklenmiyor.".to_string())
    }
}

#[cfg(target_os = "linux")]
fn linux_adb_install_base_command() -> Result<InstallCommand, String> {
    if command_path("pacman").is_some() {
        return Ok(InstallCommand {
            manager: "pacman".to_string(),
            program: "pacman".to_string(),
            args: vec![
                "-S".to_string(),
                "--needed".to_string(),
                "--noconfirm".to_string(),
                "android-tools".to_string(),
            ],
        });
    }
    if command_path("apt-get").is_some() {
        return Ok(InstallCommand {
            manager: "apt-get".to_string(),
            program: "apt-get".to_string(),
            args: vec![
                "install".to_string(),
                "-y".to_string(),
                "android-tools-adb".to_string(),
                "android-sdk-platform-tools-common".to_string(),
            ],
        });
    }
    if command_path("dnf").is_some() {
        return Ok(InstallCommand {
            manager: "dnf".to_string(),
            program: "dnf".to_string(),
            args: vec![
                "install".to_string(),
                "-y".to_string(),
                "android-tools".to_string(),
            ],
        });
    }
    if command_path("yum").is_some() {
        return Ok(InstallCommand {
            manager: "yum".to_string(),
            program: "yum".to_string(),
            args: vec![
                "install".to_string(),
                "-y".to_string(),
                "android-tools".to_string(),
            ],
        });
    }
    if command_path("zypper").is_some() {
        return Ok(InstallCommand {
            manager: "zypper".to_string(),
            program: "zypper".to_string(),
            args: vec![
                "--non-interactive".to_string(),
                "install".to_string(),
                "android-tools".to_string(),
            ],
        });
    }
    if command_path("apk").is_some() {
        return Ok(InstallCommand {
            manager: "apk".to_string(),
            program: "apk".to_string(),
            args: vec!["add".to_string(), "android-tools".to_string()],
        });
    }

    Err("Desteklenen paket yöneticisi bulunamadı. Arch/pacman, Debian/apt-get, Fedora/dnf, RHEL/yum, openSUSE/zypper veya Alpine/apk gerekli.".to_string())
}

#[cfg(target_os = "linux")]
fn elevate_linux_install_command(base: InstallCommand) -> InstallCommand {
    if is_effective_root() {
        return base;
    }
    if command_path("pkexec").is_some() {
        let mut args = Vec::with_capacity(base.args.len() + 1);
        args.push(base.program);
        args.extend(base.args);
        return InstallCommand {
            manager: format!("pkexec+{}", base.manager),
            program: "pkexec".to_string(),
            args,
        };
    }
    if command_path("sudo").is_some() {
        let mut args = Vec::with_capacity(base.args.len() + 1);
        args.push(base.program);
        args.extend(base.args);
        return InstallCommand {
            manager: format!("sudo+{}", base.manager),
            program: "sudo".to_string(),
            args,
        };
    }
    InstallCommand {
        manager: base.manager,
        program: base.program,
        args: base.args,
    }
}

#[cfg(target_os = "linux")]
fn is_effective_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .as_deref()
        == Some("0")
}

fn run_adb_devices_output() -> Result<std::process::Output, String> {
    Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .map_err(|err| {
            let msg = if err.kind() == io::ErrorKind::NotFound {
                "ADB bulunamadi".to_string()
            } else {
                format!("ADB cihaz listesi alinamadi: {err}")
            };
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:adb",
                format!("ADB devices komutu basarisiz: {}", msg),
            );
            msg
        })
}

/// Seri numarası verilen cihazda kısa ADB komutu çalıştırır.
pub(super) fn run_adb_command(serial: &str, args: &[&str]) -> Result<String, String> {
    let cmd_str = args.join(" ");
    crate::logging::runtime_log(
        crate::logging::LogLevel::Debug,
        "android:adb:cmd",
        format!("ADB KOMUT GONDERILDI [{}] → adb {}", serial, cmd_str),
    );

    let output = Command::new("adb")
        .args(["-s", serial])
        .args(args)
        .output()
        .map_err(|err| {
            let msg = if err.kind() == io::ErrorKind::NotFound {
                "ADB bulunamadi".to_string()
            } else {
                format!("ADB komutu calistirilamadi: {err}")
            };
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:adb:cmd",
                format!(
                    "ADB KOMUT HATA [{}] → adb {} | Hata: {}",
                    serial, cmd_str, msg
                ),
            );
            msg
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = first_non_empty(&stderr)
            .or_else(|| first_non_empty(&stdout))
            .unwrap_or_else(|| "ADB komutu basarisiz oldu".to_string());

        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:adb:cmd",
            format!(
                "ADB KOMUT BASARISIZ [{}] → adb {} | HTTP/Cikis: {}",
                serial, cmd_str, detail
            ),
        );
        return Err(detail);
    }

    let result = String::from_utf8_lossy(&output.stdout).into_owned();
    let result_preview = result.trim();
    let short_res = if result_preview.len() > 120 {
        format!(
            "{}... (toplam {} karakter)",
            &result_preview[..120],
            result_preview.len()
        )
    } else {
        result_preview.to_string()
    };

    crate::logging::runtime_log(
        crate::logging::LogLevel::Debug,
        "android:adb:cmd",
        format!(
            "ADB KOMUT BASARILI [{}] → adb {} | Yanit: {}",
            serial, cmd_str, short_res
        ),
    );
    Ok(result)
}

/// Uzayabilecek ADB komutlarını zaman aşımıyla çalıştırır.
pub(super) fn run_adb_command_timeout(
    serial: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let mut child = Command::new("adb")
        .args(["-s", serial])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                "ADB bulunamadi".to_string()
            } else {
                format!("ADB komutu calistirilamadi: {err}")
            }
        })?;

    let started = std::time::Instant::now();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            let output = child
                .wait_with_output()
                .map_err(|err| format!("ADB komutu sonucu alinamadi: {err}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Err(first_non_empty(&stderr)
                    .or_else(|| first_non_empty(&stdout))
                    .unwrap_or_else(|| "ADB komutu basarisiz oldu".to_string()));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "ADB komutu zaman asimina ugradi: {}s",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Dosya üreten veya dosya çeken ADB komutunu başarı/hata olarak çalıştırır.
pub(super) fn run_adb_file_command(serial: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new("adb")
        .args(["-s", serial])
        .args(args)
        .output()
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                "ADB bulunamadi".to_string()
            } else {
                format!("ADB komutu calistirilamadi: {err}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = first_non_empty(&stderr)
            .or_else(|| first_non_empty(&stdout))
            .unwrap_or_else(|| "ADB dosya komutu basarisiz oldu".to_string());
        return Err(detail);
    }
    Ok(())
}

/// Dosya ADB komutlarını zaman aşımı korumasıyla çalıştırır.
pub(super) fn run_adb_file_command_timeout(
    serial: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(), String> {
    run_adb_command_timeout(serial, args, timeout).map(|_| ())
}

/// adb version çıktısından sürüm satırını ayıklar.
fn parse_adb_version(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            line.starts_with("Android Debug Bridge version") || line.starts_with("Version ")
        })
        .map(ToOwned::to_owned)
}

/// adb version çıktısından kurulu binary yolunu ayıklar.
fn parse_adb_path(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("Installed as ").map(ToOwned::to_owned))
}

/// Platforma göre komutun PATH içindeki gerçek yolunu bulur.
fn command_path(command: &str) -> Option<String> {
    let output = if cfg!(windows) {
        Command::new("where").arg(command).output().ok()?
    } else {
        Command::new("sh")
            .args(["-c", &format!("command -v {command}")])
            .output()
            .ok()?
    };
    if !output.status.success() {
        return None;
    }
    first_non_empty(&String::from_utf8_lossy(&output.stdout))
}

/// Çok satırlı çıktıdan ilk boş olmayan satırı döndürür.
pub(super) fn first_non_empty(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn shell_display_arg(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '+'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

/// adb devices çıktısını cihaz listesine çevirir.
fn parse_adb_devices(output: &str) -> Vec<AndroidDevice> {
    output.lines().filter_map(parse_adb_device_line).collect()
}

/// adb devices -l içindeki tek satırı AndroidDevice modeline dönüştürür.
fn parse_adb_device_line(line: &str) -> Option<AndroidDevice> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("List of devices")
        || trimmed.starts_with("* daemon")
    {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let serial = parts.next()?.to_string();
    let state = parts.next().unwrap_or("unknown").to_string();
    let mut model = None;
    let mut product = None;
    let mut device = None;
    let mut transport_id = None;

    for part in parts {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        match key {
            "model" => model = Some(value.replace('_', " ")),
            "product" => product = Some(value.to_string()),
            "device" => device = Some(value.to_string()),
            "transport_id" => transport_id = Some(value.to_string()),
            _ => {}
        }
    }

    Some(AndroidDevice {
        serial,
        state,
        model,
        product,
        device,
        transport_id,
        raw: trimmed.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices_output() {
        let devices = parse_adb_devices(
            "List of devices attached\n\
             emulator-5554 device product:sdk_gphone_x86 model:sdk_gphone_x86 device:generic_x86 transport_id:1\n\
             R58M123ABC unauthorized usb:1-2 product:a52 model:SM_A525F device:a52q transport_id:2\n",
        );

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "emulator-5554");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].model.as_deref(), Some("sdk gphone x86"));
        assert_eq!(devices[1].state, "unauthorized");
        assert_eq!(devices[1].transport_id.as_deref(), Some("2"));
    }
}

#[cfg(windows)]
fn refresh_windows_path() {
    let mut paths = Vec::new();

    // HKCU Path
    if let Ok(output) = Command::new("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(path_val) = parse_reg_path_output(&stdout) {
                paths.push(path_val);
            }
        }
    }

    // HKLM Path
    if let Ok(output) = Command::new("reg")
        .args([
            "query",
            "HKLM\\System\\CurrentControlSet\\Control\\Session Manager\\Environment",
            "/v",
            "Path",
        ])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(path_val) = parse_reg_path_output(&stdout) {
                paths.push(path_val);
            }
        }
    }

    // Winget links default directory
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let winget_links = format!("{}\\Microsoft\\WinGet\\Links", localappdata);
        if std::path::Path::new(&winget_links).exists() {
            paths.push(winget_links);
        }
    }

    if !paths.is_empty() {
        if let Ok(current_path) = std::env::var("PATH") {
            let mut all_paths = paths;
            // Eger listedeki yollar zaten PATH icinde yoksa ekle
            let current_split: std::collections::HashSet<&str> = current_path.split(';').collect();
            let mut to_add = Vec::new();
            for p in &all_paths {
                if !current_split.contains(p.as_str()) {
                    to_add.push(p.as_str());
                }
            }
            if !to_add.is_empty() {
                to_add.push(&current_path);
                let merged = to_add.join(";");
                std::env::set_var("PATH", merged);
            }
        }
    }
}

#[cfg(windows)]
fn parse_reg_path_output(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("path") {
            // Path    REG_EXPAND_SZ    C:\Users\...
            let parts: Vec<&str> = trimmed
                .splitn(3, |c: char| c.is_whitespace())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() >= 3 {
                return Some(parts[2].to_string());
            }
        }
    }
    None
}
