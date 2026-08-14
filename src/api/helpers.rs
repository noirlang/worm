//! Yetki yükseltme, dosya indirme, JSON yardımcıları ve ortak API araçlarını içerir.
use chrono::Local;
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Komut stderr çıktısını yoksa fallback mesajını döndürür.
pub fn command_error_message(output: &std::process::Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        fallback.to_string()
    } else {
        stderr
    }
}

/// Sürecin root/admin yetkisiyle çalışıp çalışmadığını kontrol eder.
pub fn process_is_root() -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::geteuid() == 0 }
    }

    #[cfg(windows)]
    {
        crate::ram::is_root_or_admin()
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

/// Yetkili helper sürecinin sade durum bilgisidir.
pub struct ElevatedExitStatus {
    success: bool,
    code: Option<i32>,
}

impl ElevatedExitStatus {
    /// Helper başarılı tamamlandı mı diye kontrol eder.
    pub fn success(&self) -> bool {
        self.success
    }

    /// Platformun döndürebildiği çıkış kodunu taşır.
    pub fn code(&self) -> Option<i32> {
        self.code
    }
}

/// Linux pkexec/sudo veya Windows UAC ile başlatılmış helper sürecini sarmalar.
pub struct ElevatedChild {
    method: &'static str,
    #[cfg(target_os = "linux")]
    child: Child,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl ElevatedChild {
    /// Yetki yükseltme mekanizmasının adını döndürür.
    pub fn method(&self) -> &'static str {
        self.method
    }

    /// Helper süreci bittiyse durumunu, devam ediyorsa None döndürür.
    pub fn try_wait(&mut self) -> Result<Option<ElevatedExitStatus>, String> {
        #[cfg(target_os = "linux")]
        {
            return self
                .child
                .try_wait()
                .map(|status| {
                    status.map(|status| ElevatedExitStatus {
                        success: status.success(),
                        code: status.code(),
                    })
                })
                .map_err(|err| err.to_string());
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::STILL_ACTIVE;
            use windows_sys::Win32::System::Threading::GetExitCodeProcess;

            let mut code = 0_u32;
            let ok = unsafe { GetExitCodeProcess(self.handle, &mut code) };
            if ok == 0 {
                return Err(format!(
                    "Windows UAC helper durumu okunamadi (Win32 hata kodu: {})",
                    unsafe { windows_sys::Win32::Foundation::GetLastError() }
                ));
            }
            if code == STILL_ACTIVE as u32 {
                Ok(None)
            } else {
                Ok(Some(ElevatedExitStatus {
                    success: code == 0,
                    code: Some(code as i32),
                }))
            }
        }

        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Ok(Some(ElevatedExitStatus {
                success: false,
                code: None,
            }))
        }
    }

    /// Helper sürecinin bitmesini bekler.
    pub fn wait(&mut self) -> Result<ElevatedExitStatus, String> {
        #[cfg(target_os = "linux")]
        {
            return self
                .child
                .wait()
                .map(|status| ElevatedExitStatus {
                    success: status.success(),
                    code: status.code(),
                })
                .map_err(|err| err.to_string());
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::WAIT_FAILED;
            use windows_sys::Win32::System::Threading::{
                GetExitCodeProcess, INFINITE, WaitForSingleObject,
            };

            let wait = unsafe { WaitForSingleObject(self.handle, INFINITE) };
            if wait == WAIT_FAILED {
                return Err(format!(
                    "Windows UAC helper beklenemedi (Win32 hata kodu: {})",
                    unsafe { windows_sys::Win32::Foundation::GetLastError() }
                ));
            }
            let mut code = 1_u32;
            let ok = unsafe { GetExitCodeProcess(self.handle, &mut code) };
            if ok == 0 {
                return Err(format!(
                    "Windows UAC helper cikis kodu okunamadi (Win32 hata kodu: {})",
                    unsafe { windows_sys::Win32::Foundation::GetLastError() }
                ));
            }
            Ok(ElevatedExitStatus {
                success: code == 0,
                code: Some(code as i32),
            })
        }

        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Ok(ElevatedExitStatus {
                success: false,
                code: None,
            })
        }
    }

    /// Uzun süren helper sürecini iptal eder.
    pub fn kill(&mut self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            return self.child.kill().map_err(|err| err.to_string());
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::TerminateProcess;
            let ok = unsafe { TerminateProcess(self.handle, 1) };
            if ok == 0 {
                Err(format!(
                    "Windows UAC helper sonlandirilamadi (Win32 hata kodu: {})",
                    unsafe { windows_sys::Win32::Foundation::GetLastError() }
                ))
            } else {
                Ok(())
            }
        }

        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Ok(())
        }
    }

    /// Helper başarısız tamamlandığında stderr/kod bilgisinden açıklayıcı mesaj üretir.
    pub fn failure_message(&mut self, status: &ElevatedExitStatus) -> String {
        #[cfg(target_os = "linux")]
        let stderr = {
            let mut text = String::new();
            if let Some(mut pipe) = self.child.stderr.take() {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        };

        #[cfg(not(target_os = "linux"))]
        let stderr = String::new();

        describe_elevation_failure(self.method, status.code(), &stderr)
    }
}

#[cfg(windows)]
impl Drop for ElevatedChild {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// Linux pkexec/sudo veya Windows UAC ile aynı binary'yi yetkili helper olarak başlatır.
pub fn spawn_elevated_helper(args: &[String]) -> Result<ElevatedChild, String> {
    #[cfg(target_os = "linux")]
    {
        spawn_linux_elevated_helper(args)
    }

    #[cfg(windows)]
    {
        spawn_windows_elevated_helper(args)
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = args;
        Err("yetki yükseltme bu platformda desteklenmiyor".to_string())
    }
}

#[cfg(target_os = "linux")]
/// Linux'ta root helper'ı sudo askpass veya pkexec ile başlatır.
fn spawn_linux_elevated_helper(args: &[String]) -> Result<ElevatedChild, String> {
    let exe = elevated_helper_executable()?;
    if process_is_root() {
        let child = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("root helper baslatilamadi: {err}"))?;
        return Ok(ElevatedChild {
            method: "direct-root",
            child,
        });
    }

    let forced = std::env::var("AMELE_ELEVATION_METHOD")
        .ok()
        .map(|value| value.to_ascii_lowercase());
    let mut errors = Vec::new();
    let mut methods = Vec::new();

    match forced.as_deref() {
        Some("sudo") | Some("sudo-askpass") => methods.push("sudo-askpass"),
        Some("pkexec") | Some("polkit") => methods.push("pkexec"),
        Some(other) => errors.push(format!("Bilinmeyen AMELE_ELEVATION_METHOD: {other}")),
        None => {
            if command_in_path("sudo") && linux_gui_askpass_available() {
                methods.push("sudo-askpass");
            }
            if command_in_path("pkexec") {
                methods.push("pkexec");
            }
            if methods.is_empty() && command_in_path("sudo") {
                methods.push("sudo-askpass");
            }
        }
    }

    for method in methods {
        let result = match method {
            "sudo-askpass" => spawn_linux_sudo_askpass(&exe, args),
            "pkexec" => spawn_linux_pkexec(&exe, args),
            _ => unreachable!("known elevation method"),
        };
        match result {
            Ok(child) => return Ok(child),
            Err(err) => errors.push(err),
        }
    }

    if errors.is_empty() {
        errors.push("sudo/pkexec bulunamadi".to_string());
    }

    Err(format!(
        "Linux yetki yükseltme başlatılamadı.\n{}\nÇözüm: pkexec/polkit agent veya sudo için zenity/kdialog/ssh-askpass kurun; terminalden `sudo -v` ile yetkiyi doğrulayıp tekrar deneyin.",
        errors.join("\n")
    ))
}

#[cfg(target_os = "linux")]
/// sudo -A ile grafik parola penceresi üzerinden helper başlatır.
fn spawn_linux_sudo_askpass(exe: &Path, args: &[String]) -> Result<ElevatedChild, String> {
    if !command_in_path("sudo") {
        return Err("sudo bulunamadi".to_string());
    }
    let askpass = ensure_sudo_askpass_script()?;
    let child = Command::new("sudo")
        .arg("-A")
        .arg("-p")
        .arg("Amele Forensic Tool yetkisi gerekiyor: ")
        .arg(exe)
        .args(args)
        .env("SUDO_ASKPASS", askpass)
        .env("SUDO_ASKPASS_REQUIRE", "force")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("sudo askpass baslatilamadi: {err}"))?;
    Ok(ElevatedChild {
        method: "sudo-askpass",
        child,
    })
}

#[cfg(target_os = "linux")]
/// pkexec/polkit üzerinden helper başlatır.
fn spawn_linux_pkexec(exe: &Path, args: &[String]) -> Result<ElevatedChild, String> {
    if !command_in_path("pkexec") {
        return Err("pkexec bulunamadi".to_string());
    }
    let child = Command::new("pkexec")
        .arg(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("pkexec baslatilamadi: {err}"))?;
    Ok(ElevatedChild {
        method: "pkexec",
        child,
    })
}

#[cfg(target_os = "linux")]
/// Sistemde sudo askpass penceresi açabilecek araç var mı kontrol eder.
fn linux_gui_askpass_available() -> bool {
    std::env::var_os("SUDO_ASKPASS")
        .map(PathBuf::from)
        .is_some_and(|path| path.is_file())
        || [
            "zenity",
            "kdialog",
            "ssh-askpass",
            "x11-ssh-askpass",
            "lxqt-openssh-askpass",
        ]
        .iter()
        .any(|program| command_in_path(program))
}

#[cfg(target_os = "linux")]
/// sudo -A için geçici ve sabit bir askpass betiği hazırlar.
fn ensure_sudo_askpass_script() -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(path) = std::env::var_os("SUDO_ASKPASS").map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }

    let script = std::env::temp_dir().join("amele-sudo-askpass.sh");
    let body = r#"#!/bin/sh
prompt="${SUDO_ASKPASS_PROMPT:-Amele Forensic Tool yetkisi gerekiyor}"
if command -v zenity >/dev/null 2>&1; then
  exec zenity --password --title="Amele Forensic Tool" --text="$prompt"
fi
if command -v kdialog >/dev/null 2>&1; then
  exec kdialog --password "$prompt"
fi
if command -v ssh-askpass >/dev/null 2>&1; then
  exec ssh-askpass "$prompt"
fi
if command -v x11-ssh-askpass >/dev/null 2>&1; then
  exec x11-ssh-askpass "$prompt"
fi
if command -v lxqt-openssh-askpass >/dev/null 2>&1; then
  exec lxqt-openssh-askpass "$prompt"
fi
exit 1
"#;
    fs::write(&script, body).map_err(|err| format!("sudo askpass betigi yazilamadi: {err}"))?;
    let mut permissions = fs::metadata(&script)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).map_err(|err| err.to_string())?;
    Ok(script)
}

#[cfg(target_os = "linux")]
/// PATH içinde program var mı hızlıca kontrol eder.
fn command_in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

#[cfg(windows)]
/// Windows'ta native ShellExecuteEx runas ile UAC penceresi açar.
fn spawn_windows_elevated_helper(args: &[String]) -> Result<ElevatedChild, String> {
    use windows_sys::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let parameters = quote_windows_arguments(args);
    let verb = wide_null("runas");
    let file = wide_null(&exe.to_string_lossy());
    let params = wide_null(&parameters);

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_SHOWNORMAL;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_CANCELLED {
            return Err(
                "Windows UAC penceresi kullanıcı tarafından iptal edildi. İmaj/RAM işlemi için Evet seçilmeli veya Amele yönetici olarak başlatılmalı."
                    .to_string(),
            );
        }
        return Err(format!(
            "Windows UAC başlatılamadı (Win32 hata kodu: {code}). UAC kapalı, güvenlik ilkesi engelliyor veya uygulama yolu çalıştırılamıyor olabilir."
        ));
    }

    if info.hProcess.is_null() {
        return Err("Windows UAC helper process handle alınamadı".to_string());
    }

    Ok(ElevatedChild {
        method: "windows-uac",
        handle: info.hProcess,
    })
}

#[cfg(windows)]
/// Windows komut satırı argümanlarını CommandLineToArgvW uyumlu quote eder.
fn quote_windows_arguments(args: &[String]) -> String {
    args.iter()
        .map(|arg| quote_windows_argument(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn quote_windows_argument(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if !arg.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        return arg.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(ch);
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "linux")]
/// AppImage içindeyken doğru helper binary yolunu döndürür.
pub fn elevated_helper_executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    std::env::current_exe().map_err(|err| err.to_string())
}

/// Yetki mekanizmasından gelen ham stderr/kod bilgisini okunabilir mesaja çevirir.
pub fn describe_elevation_failure(method: &str, code: Option<i32>, stderr: &str) -> String {
    let raw = stderr.trim();
    let lower = raw.to_lowercase();
    let code_text = code
        .map(|value| format!(" Çıkış kodu: {value}."))
        .unwrap_or_default();

    let reason = if method == "sudo-askpass" {
        if lower.contains("incorrect password") || lower.contains("sorry, try again") {
            "sudo parolası hatalı girildi veya deneme hakkı bitti."
        } else if lower.contains("no askpass program")
            || lower.contains("askpass")
            || lower.contains("a password is required")
        {
            "sudo parola penceresi açılamadı; sistemde askpass aracı yok veya grafik oturuma erişemiyor."
        } else if lower.contains("not in the sudoers") {
            "kullanıcı sudo yetkisine sahip değil."
        } else {
            "sudo ile yetki yükseltme tamamlanamadı."
        }
    } else if method == "pkexec" {
        if lower.contains("no authentication agent")
            || lower.contains("authentication agent")
            || lower.contains("polkit")
        {
            "pkexec için çalışan polkit authentication agent bulunamadı; bu yüzden parola penceresi açılamadı."
        } else if lower.contains("dismissed")
            || lower.contains("cancel")
            || lower.contains("not authorized")
        {
            "pkexec yetki isteği iptal edildi veya kullanıcı yetkili değil."
        } else {
            "pkexec ile yetki yükseltme tamamlanamadı."
        }
    } else if method.starts_with("windows") {
        "Windows UAC isteği tamamlanamadı veya helper yönetici olarak çalışmadı."
    } else {
        "yetki yükseltme tamamlanamadı."
    };

    let mut message = format!("Yetki yükseltme başarısız ({method}).{code_text} Neden: {reason}");
    if !raw.is_empty() {
        message.push_str(&format!("\nAyrıntı: {raw}"));
    }
    message.push_str(
        "\nÇözüm: Linux'ta sudo/pkexec parola penceresini onaylayın; pencere açılmıyorsa polkit agent veya zenity/kdialog/ssh-askpass kurun. Windows'ta UAC penceresini onaylayın veya Amele'u yönetici olarak başlatın.",
    );
    message
}

/// Yetkili helper sürecini başlatır ve tamamlanmasını bekler.
pub fn run_elevated_helper_wait(args: &[String]) -> Result<(), String> {
    let mut child = spawn_elevated_helper(args)?;
    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(child.failure_message(&status))
    }
}

/// URL'den dosya indirir; Windows'ta PowerShell, diğerlerinde curl kullanır.
pub fn download_file_to_path(url: &str, target: &Path, fallback: &str) -> Result<(), String> {
    #[cfg(windows)]
    let output = {
        let target_str = target.to_string_lossy();
        let ps_command = format!(
            "$ErrorActionPreference = 'Stop'; \
             [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
             $ProgressPreference = 'SilentlyContinue'; \
             $url = '{}'; $target = '{}'; \
             try {{ \
               Invoke-WebRequest -Uri $url -OutFile $target -UseBasicParsing; \
             }} catch {{ \
               $verifiedError = $_.Exception.Message; \
               [Net.ServicePointManager]::ServerCertificateValidationCallback = {{ $true }}; \
               try {{ \
                 Invoke-WebRequest -Uri $url -OutFile $target -UseBasicParsing; \
               }} catch {{ \
                 throw \"TLS verified download failed: $verifiedError; certificate fallback failed: $($_.Exception.Message)\"; \
               }} \
             }}",
            url.replace('\'', "''"),
            target_str.replace('\'', "''"),
        );
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(&ps_command)
            .output()
    };

    #[cfg(not(windows))]
    let output = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("-o")
        .arg(target)
        .arg(url)
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(command_error_message(&output, fallback)),
        Err(err) => Err(err.to_string()),
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    crate::hash::calculate_file_hash(path, crate::hash::HashAlgorithm::Sha256)
        .map_err(|e| e.to_string())
}

/// Geçici helper dosyaları için çakışmayan zaman damgalı ad kökü üretir.
pub fn helper_file_stem(prefix: &str) -> String {
    format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        Local::now().format("%Y%m%d%H%M%S%3f")
    )
}

/// JSON değeri pretty formatla belirtilen dosyaya yazar.
pub fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

/// Helper kontrol dosyasına running/pause/stop durumunu yazar.
pub fn write_helper_control_state(path: &Path, state: &str) -> Result<(), String> {
    write_json_file(path, &json!({ "state": state }))
}

/// Helper sonucunu JSON olarak okur.
pub fn read_helper_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|err| err.to_string())?)
        .map_err(|err| err.to_string())
}

/// Helper hata dosyasından hata metnini okur.
pub fn read_helper_error(path: &Path) -> Option<String> {
    read_helper_json(path).ok().and_then(|value| {
        value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

/// Helper ilerleme dosyasından done/total/message değerlerini okur.
pub fn read_helper_progress(path: &Path) -> Option<(u64, u64, String)> {
    let value = read_helper_json(path).ok()?;
    let done = value
        .get("done")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Imaj alma sürüyor")
        .to_string();
    Some((done, total, message))
}

/// Geçici helper dosyalarını sessizce temizler.
pub fn cleanup_helper_files(paths: &[&Path]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
/// Yetkili helper çıktılarının sahibini düzeltmek için çağıran UID değerini döndürür.
pub fn helper_owner_uid() -> Option<u32> {
    Some(unsafe { libc::geteuid() })
}

#[cfg(not(unix))]
/// Unix dışı platformlarda helper UID bilgisi yoktur.
pub fn helper_owner_uid() -> Option<u32> {
    None
}

#[cfg(unix)]
/// Yetkili helper çıktılarının sahibini düzeltmek için çağıran GID değerini döndürür.
pub fn helper_owner_gid() -> Option<u32> {
    Some(unsafe { libc::getegid() })
}

#[cfg(not(unix))]
/// Unix dışı platformlarda helper GID bilgisi yoktur.
pub fn helper_owner_gid() -> Option<u32> {
    None
}

/// Disk listesi için yetkili helper çağırır ve sonucu parse eder.
pub fn elevated_disk_list() -> Result<Vec<crate::disk::DiskInfo>, String> {
    let output_path = std::env::temp_dir().join(format!(
        "amele-disk-list-{}-{}.json",
        std::process::id(),
        Local::now().format("%Y%m%d%H%M%S%3f")
    ));

    let run_result = run_elevated_disk_list_helper(&output_path);
    if let Err(err) = run_result {
        let _ = fs::remove_file(&output_path);
        return Err(err);
    }

    let content = fs::read_to_string(&output_path).map_err(|err| err.to_string())?;
    let _ = fs::remove_file(&output_path);
    let value: Value = serde_json::from_str(&content).map_err(|err| err.to_string())?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("elevated disk list failed")
            .to_string());
    }
    serde_json::from_value(
        value
            .get("disks")
            .cloned()
            .unwrap_or(Value::Array(Vec::new())),
    )
    .map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
/// Linux disk listeleme helper komutunu çalıştırır.
fn run_elevated_disk_list_helper(output_path: &Path) -> Result<(), String> {
    run_elevated_helper_wait(&[
        "disk-list-helper".to_string(),
        output_path.to_string_lossy().into_owned(),
    ])
}

#[cfg(windows)]
/// Windows disk listeleme helper komutunu çalıştırır.
fn run_elevated_disk_list_helper(output_path: &Path) -> Result<(), String> {
    run_elevated_helper_wait(&[
        "disk-list-helper".to_string(),
        output_path.to_string_lossy().into_owned(),
    ])
}

#[cfg(not(any(target_os = "linux", windows)))]
/// Desteklenmeyen platformlarda yetkili disk listelemeyi hata olarak döndürür.
fn run_elevated_disk_list_helper(_output_path: &Path) -> Result<(), String> {
    Err("yetki yükseltmeli disk listeleme bu platformda desteklenmiyor".to_string())
}
