//! Masaüstü entegrasyonu gerektiren URL açma ve dosya seçici API uçlarını içerir.
use serde::Deserialize;
use serde_json::json;
use std::process::{Command, Stdio};

use crate::server::{Response, json_error, json_ok};

/// Developer konsolunu sistem tarayıcısında açar (native WebView'da window.open çalışmaz).
pub fn open_dev_console_endpoint() -> Response {
    let port = crate::api::current_server_port();
    let url = format!("http://127.0.0.1:{}/?route=devlogs", port);
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "api:devconsole",
        format!("Developer konsolu aciliyor: {}", url),
    );
    match open_external_url(&url) {
        Ok(()) => json_ok(json!({ "opened": true, "url": url })),
        Err(err) => json_error(500, err),
    }
}

/// Güvenli harici URL'yi sistem tarayıcısında açar.
pub fn open_url_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct OpenUrlRequest {
        url: String,
    }

    let request: OpenUrlRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    let url = match validate_external_url(&request.url) {
        Ok(url) => url,
        Err(err) => return json_error(400, err),
    };

    open_external_url(&url)
        .map(|()| json_ok(json!({ "opened": true })))
        .unwrap_or_else(|err| json_error(500, err))
}

/// Native dosya/klasör seçici açar.
pub fn pick_path_endpoint(directory: bool) -> Response {
    match pick_path(directory) {
        Ok(Some(path)) => json_ok(json!({ "path": path })),
        Ok(None) => json_error(499, "selection cancelled"),
        Err(err) => json_error(500, err),
    }
}

/// Platforma göre dosya veya klasör seçici çalıştırır.
fn pick_path(directory: bool) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        pick_path_windows(directory)
    }

    #[cfg(not(windows))]
    {
        pick_path_unix(directory)
    }
}

#[cfg(not(windows))]
/// Unix ortamında zenity/kdialog/yad ile dosya seçici açar.
fn pick_path_unix(directory: bool) -> Result<Option<String>, String> {
    let candidates: &[(&str, &[&str])] = if directory {
        &[
            ("zenity", &["--file-selection", "--directory"]),
            ("kdialog", &["--getexistingdirectory"]),
        ]
    } else {
        &[
            ("zenity", &["--file-selection"]),
            ("kdialog", &["--getopenfilename"]),
        ]
    };

    let mut last_error = String::new();
    for (program, args) in candidates {
        match Command::new(program).args(*args).output() {
            Ok(output) if output.status.success() => {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if path.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(path));
            }
            Ok(output) => {
                if output.status.code() == Some(1) {
                    return Ok(None);
                }
                last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            }
            Err(err) => last_error = err.to_string(),
        }
    }

    Err(if last_error.is_empty() {
        "no file picker command found".to_string()
    } else {
        last_error
    })
}

#[cfg(windows)]
/// Windows PowerShell ile dosya veya klasör seçici açar.
fn pick_path_windows(directory: bool) -> Result<Option<String>, String> {
    let script = if directory {
        r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
  Write-Output $dialog.SelectedPath
  exit 0
}
exit 1
"#
    } else {
        r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.CheckFileExists = $true
$dialog.Multiselect = $false
$dialog.Filter = 'All files (*.*)|*.*'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
  Write-Output $dialog.FileName
  exit 0
}
exit 1
"#
    };

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-STA")
        .arg("-Command")
        .arg(script)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("Windows file picker baslatilamadi: {err}"))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            "Windows file picker acilamadi".to_string()
        } else {
            error
        })
    }
}

/// Platforma göre harici URL açma komutunu çalıştırır.
fn open_external_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let openers: &[(&str, &[&str])] = &[("xdg-open", &[url]), ("gio", &["open", url])];
        for (program, args) in openers {
            if Command::new(program)
                .args(*args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        Err("external link opener could not be started".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("external link opener could not be started: {err}"))
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("external link opener could not be started: {err}"))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = url;
        Err("external links are not supported on this platform".to_string())
    }
}

/// Sadece izin verilen URL şemalarını kabul eder.
fn validate_external_url(value: &str) -> Result<String, String> {
    let url = value.trim();
    if url.is_empty() {
        return Err("url is required".to_string());
    }
    if url.chars().any(char::is_control) {
        return Err("url contains invalid characters".to_string());
    }

    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
    {
        Ok(url.to_string())
    } else {
        Err("only http, https and mailto links can be opened".to_string())
    }
}
