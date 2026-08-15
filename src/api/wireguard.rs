//! WireGuard yapılandırma dosyası üretimi ve bağlantı API uçlarını yönetir.
use super::wireguard_manager;
use crate::server::{Response, json_error, json_ok};
use crate::wireguard::{self, WireGuardConfig, WireGuardManager};
use serde::Deserialize;
use serde_json::json;
use std::sync::MutexGuard;

#[derive(Deserialize)]
struct WireGuardFileRequest {
    config_file: String,
}

fn lock_manager() -> Result<MutexGuard<'static, WireGuardManager>, Response> {
    wireguard_manager()
        .lock()
        .map_err(|_| json_error(500, "wireguard manager lock failed"))
}

/// WireGuard config isteğini doğrular ve config dosyasını üretir.
pub fn wireguard_config_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct WireGuardConfigRequest {
        config_file: String,
        private_key: Option<String>,
        public_key: Option<String>,
        endpoint: String,
        allowed_ips: Option<String>,
        address: Option<String>,
        dns: Option<String>,
        keepalive: Option<u16>,
    }

    let request: WireGuardConfigRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    if request.config_file.trim().is_empty() {
        return json_error(400, "config_file is required");
    }
    if request.endpoint.trim().is_empty() {
        return json_error(400, "endpoint is required");
    }
    let config = WireGuardConfig {
        private_key: request.private_key.as_deref().unwrap_or_default().trim(),
        public_key: request.public_key.as_deref().unwrap_or_default().trim(),
        endpoint: request.endpoint.trim(),
        allowed_ips: request.allowed_ips.as_deref().unwrap_or("0.0.0.0/0").trim(),
        address: request.address.as_deref().unwrap_or("10.0.0.2/24").trim(),
        dns: request.dns.as_deref().unwrap_or("1.1.1.1").trim(),
        keepalive: request.keepalive.unwrap_or(25),
    };

    match wireguard::create_config(request.config_file.trim(), &config) {
        Ok(path) => json_ok(json!({ "path": path })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Seçilen WireGuard config dosyasıyla bağlantıyı başlatır.
pub fn wireguard_start_endpoint(body: &[u8]) -> Response {
    let request: WireGuardFileRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    if request.config_file.trim().is_empty() {
        return json_error(400, "config_file is required");
    }
    let mut guard = match lock_manager() {
        Ok(g) => g,
        Err(resp) => return resp,
    };
    match guard.start(request.config_file.trim()) {
        Ok(()) => json_ok(json!({
            "active": guard.is_active(),
            "config_file": guard.config_file,
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Aktif WireGuard bağlantısını durdurur.
pub fn wireguard_stop_endpoint() -> Response {
    let mut guard = match lock_manager() {
        Ok(g) => g,
        Err(resp) => return resp,
    };
    match guard.stop() {
        Ok(()) => json_ok(json!({ "active": guard.is_active() })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// WireGuard manager'ın mevcut aktiflik durumunu döndürür.
pub fn wireguard_status_endpoint() -> Response {
    let guard = match lock_manager() {
        Ok(g) => g,
        Err(resp) => return resp,
    };
    json_ok(json!({
        "interface_name": guard.interface_name,
        "config_file": guard.config_file,
        "active": guard.active,
    }))
}
