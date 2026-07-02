//! Developer modu log ve sistem tanılama API uçlarını içerir.
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::{Response, json_error, json_ok};

use super::{acquisition_jobs, process_is_root};

#[derive(Deserialize)]
/// UI veya frontend tarafının developer log'a eklemek istediği satırı taşır.
struct DeveloperLogRequest {
    level: Option<String>,
    scope: Option<String>,
    message: String,
}

/// Developer mod penceresinin okuyacağı runtime log, job ve sistem özetini döndürür.
pub fn developer_logs_endpoint() -> Response {
    json_ok(json!({
        "logs": crate::logging::runtime_logs(1000),
        "log_file": crate::logging::runtime_log_file_path(),
        "jobs": developer_job_snapshot(),
        "system": developer_system_snapshot(),
    }))
}

/// Frontend tarafındaki hata ve kritik olayları backend runtime log'una işler.
pub fn developer_log_endpoint(body: &[u8]) -> Response {
    let request: DeveloperLogRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let level = match request
        .level
        .as_deref()
        .unwrap_or("info")
        .to_ascii_lowercase()
        .as_str()
    {
        "error" => crate::logging::LogLevel::Error,
        "warn" | "warning" => crate::logging::LogLevel::Warn,
        "debug" => crate::logging::LogLevel::Debug,
        _ => crate::logging::LogLevel::Info,
    };
    let scope = request.scope.unwrap_or_else(|| "ui".to_string());
    crate::logging::runtime_log(level, scope, request.message);
    json_ok(json!({ "ok": true }))
}

/// Developer paneli için aktif/son işlerin sade özetini üretir.
fn developer_job_snapshot() -> Vec<Value> {
    acquisition_jobs()
        .lock()
        .map(|jobs| {
            jobs.iter()
                .map(|(id, job)| {
                    json!({
                        "id": id,
                        "status": job.status,
                        "done": job.done,
                        "total": job.total,
                        "message": job.message,
                        "log_count": job.logs.len(),
                        "error": job.error,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Platform, yetki, yol ve paket bilgilerini tek yerde özetler.
fn developer_system_snapshot() -> Value {
    let env_keys = [
        "APPDIR",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_CURRENT_DESKTOP",
        "GDK_BACKEND",
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_EXEC_PATH",
        "WEBVIEW2_USER_DATA_FOLDER",
        "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
        "PATH",
        "HOME",
        "USER",
        "USERNAME",
        "SHELL",
        "LANG",
        "LC_ALL",
        "TZ",
        "LOGNAME",
        "HOSTNAME",
    ];
    let env = env_keys
        .iter()
        .map(|key| {
            let value = std::env::var(key)
                .ok()
                .map(|value| {
                    if *key == "PATH" && value.len() > 280 {
                        format!("{}...", &value[..280])
                    } else {
                        value
                    }
                })
                .unwrap_or_else(|| "(yok)".to_string());
            json!({ "key": key, "value": value })
        })
        .collect::<Vec<_>>();

    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok();
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();
    let timezone = std::env::var("TZ").ok().or_else(|| {
        std::panic::catch_unwind(|| chrono::Local::now().format("%Z").to_string()).ok()
    });

    let server_port = crate::api::current_server_port();
    let (total_memory, free_memory) = get_system_memory();

    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "family": std::env::consts::FAMILY,
        "arch": std::env::consts::ARCH,
        "pid": std::process::id(),
        "cwd": std::env::current_dir().ok(),
        "exe": std::env::current_exe().ok(),
        "ui_root": crate::server::ui_root(),
        "is_elevated": process_is_root(),
        "runtime_log_file": crate::logging::runtime_log_file_path(),
        "server_port": server_port,
        "hostname": hostname,
        "username": username,
        "timezone": timezone,
        "total_memory": total_memory,
        "free_memory": free_memory,
        "env": env,
    })
}

/// Linux /proc/meminfo'dan sistem belleği bilgisi alır.
fn get_system_memory() -> (Option<u64>, Option<u64>) {
    #[cfg(target_os = "linux")]
    {
        let content = match std::fs::read_to_string("/proc/meminfo") {
            Ok(c) => c,
            Err(_) => return (None, None),
        };
        let mut total = None;
        let mut free = None;
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("MemTotal:") {
                total = val
                    .trim()
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|kb| kb * 1024);
            } else if let Some(val) = line.strip_prefix("MemAvailable:") {
                free = val
                    .trim()
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|kb| kb * 1024);
            }
        }
        (total, free)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (None, None)
    }
}
