//! Kalıcı uygulama ayarları API uçlarını içerir.
use serde::Deserialize;
use serde_json::json;

use crate::server::{Response, json_error, json_ok};

#[derive(Deserialize)]
/// UI ayar kaydetme isteğinde tema ve dil tercihini taşır.
struct SaveSettingsRequest {
    theme: Option<String>,
    language: Option<String>,
}

/// Kalıcı uygulama ayarlarını dosyadan okur.
pub fn settings_get_endpoint() -> Response {
    let path = crate::settings::default_settings_path();
    match crate::settings::AppSettings::load(&path) {
        Ok(settings) => json_ok(json!({
            "settings": settings,
            "path": path,
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Tema ve dil tercihini kalıcı ayar dosyasına yazar.
pub fn settings_save_endpoint(body: &[u8]) -> Response {
    let request: SaveSettingsRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    let path = crate::settings::default_settings_path();
    let mut settings = match crate::settings::AppSettings::load(&path) {
        Ok(settings) => settings,
        Err(err) => return json_error(500, err.to_string()),
    };

    if let Some(theme) = request.theme.as_deref() {
        match theme {
            "dark" => settings.karanlik_tema = true,
            "light" => settings.karanlik_tema = false,
            other => return json_error(400, format!("unsupported theme: {other}")),
        }
    }

    if let Some(language) = request.language.as_deref() {
        match language {
            "tr" | "en" => settings.dil = language.to_string(),
            other => return json_error(400, format!("unsupported language: {other}")),
        }
    }

    settings.normalize();
    match settings.save(&path) {
        Ok(()) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:settings",
                format!(
                    "Ayarlar kaydedildi: theme={} language={} path={}",
                    if settings.karanlik_tema {
                        "dark"
                    } else {
                        "light"
                    },
                    settings.dil,
                    path.display()
                ),
            );
            json_ok(json!({
                "settings": settings,
                "path": path,
            }))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}
