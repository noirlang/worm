//! Yerel profil oluşturma, seçme ve çıkış API uçlarını içerir.
use serde::Deserialize;
use serde_json::json;

use crate::server::{Response, json_error, json_ok};

#[derive(Deserialize)]
/// Profil oluşturma isteğinin gövdesidir.
struct CreateProfileRequest {
    full_name: String,
    username: String,
    language: Option<String>,
    theme: Option<String>,
    open_directly: Option<bool>,
}

#[derive(Deserialize)]
/// Profil seçme isteğinin gövdesidir.
struct SelectProfileRequest {
    username: String,
    open_directly: Option<bool>,
}

#[derive(Deserialize)]
/// Online profil bağlama isteğinin gövdesidir.
struct OnlineProfileLoginRequest {
    identifier: String,
    password: String,
    language: Option<String>,
    theme: Option<String>,
    open_directly: Option<bool>,
}

/// Profil listesini ve açılış kararını döndürür.
pub fn profiles_get_endpoint() -> Response {
    match crate::profile::bootstrap_profiles() {
        Ok(state) => json_ok(json!({
            "profiles": state.profiles,
            "active_profile": state.active_profile,
            "should_prompt": state.should_prompt,
            "base_dir": state.base_dir,
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Yeni profil oluşturur, aktif eder ve profil klasörlerini hazırlar.
pub fn profile_create_endpoint(body: &[u8]) -> Response {
    let request: CreateProfileRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    match crate::profile::create_profile(
        &request.full_name,
        &request.username,
        request.language.as_deref().unwrap_or("tr"),
        request.theme.as_deref().unwrap_or("dark"),
        request.open_directly.unwrap_or(false),
    ) {
        Ok(profile) => json_ok(json!({
            "profile": profile,
            "settings_path": crate::settings::default_settings_path(),
            "case_base_dir": crate::api::default_case_base_dir(),
        })),
        Err(err) => json_error(400, err.to_string()),
    }
}

/// Var olan profili aktif eder.
pub fn profile_select_endpoint(body: &[u8]) -> Response {
    let request: SelectProfileRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    match crate::profile::select_profile(&request.username, request.open_directly.unwrap_or(false))
    {
        Ok(profile) => json_ok(json!({
            "profile": profile,
            "settings_path": crate::settings::default_settings_path(),
            "case_base_dir": crate::api::default_case_base_dir(),
        })),
        Err(err) => json_error(404, err.to_string()),
    }
}

/// Online site hesabını yerel profile bağlar ve aktif profili günceller.
pub fn profile_online_login_endpoint(body: &[u8]) -> Response {
    let request: OnlineProfileLoginRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    match crate::profile::link_online_profile(
        &request.identifier,
        &request.password,
        request.language.as_deref().unwrap_or("tr"),
        request.theme.as_deref().unwrap_or("dark"),
        request.open_directly.unwrap_or(false),
    ) {
        Ok(profile) => json_ok(json!({
            "profile": profile,
            "access": crate::profile::mobile_tools_access(),
            "settings_path": crate::settings::default_settings_path(),
            "case_base_dir": crate::api::default_case_base_dir(),
        })),
        Err(err) => json_error(401, err.to_string()),
    }
}

/// Aktif online profil bilgilerini site API'sinden yeniler.
pub fn profile_online_sync_endpoint() -> Response {
    match crate::profile::sync_active_online_profile() {
        Ok(profile) => json_ok(json!({
            "profile": profile,
            "access": crate::profile::mobile_tools_access(),
        })),
        Err(err) => json_error(400, err.to_string()),
    }
}

/// Aktif profilden online hesap bağlantısını kaldırır.
pub fn profile_online_logout_endpoint() -> Response {
    match crate::profile::disconnect_active_online_profile() {
        Ok(profile) => json_ok(json!({
            "profile": profile,
            "access": crate::profile::mobile_tools_access(),
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Android/iOS araçları için online üyelik durumunu döndürür.
pub fn profile_mobile_access_endpoint() -> Response {
    match crate::profile::require_mobile_tools_access() {
        Ok(()) => json_ok(json!({
            "access": {
                "allowed": true,
                "profile": crate::profile::active_profile(),
            }
        })),
        Err(err) => json_ok(json!({
            "access": {
                "allowed": false,
                "reason": err.to_string(),
                "profile": crate::profile::active_profile(),
            }
        })),
    }
}

/// Mobil araç API'leri için üyelik kilidini uygular.
pub fn require_mobile_tools_response() -> Option<Response> {
    match crate::profile::require_mobile_tools_access() {
        Ok(()) => None,
        Err(err) => Some(json_error(403, err.to_string())),
    }
}

/// Aktif profilden çıkar ve sonraki açılışta profil seçimini gösterir.
pub fn profile_logout_endpoint() -> Response {
    match crate::profile::logout_profile() {
        Ok(()) => json_ok(json!({ "ok": true })),
        Err(err) => json_error(500, err.to_string()),
    }
}
