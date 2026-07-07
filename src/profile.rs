//! Yerel profil, profil bazlı ayar ve vaka klasörü yönetimini sağlar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::settings;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Uygulamada şifresiz yerel hesap olarak kullanılan profil bilgisidir.
pub struct LocalProfile {
    pub username: String,
    pub full_name: String,
    pub display_name: String,
    pub language: String,
    pub theme: String,
    pub open_directly: bool,
    pub created_at: String,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
/// Tüm yerel profilleri ve son aktif profili saklayan dosya modelidir.
pub struct ProfileStore {
    pub profiles: Vec<LocalProfile>,
    pub active_username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Profil listesini UI'ye açılış kararlarıyla birlikte döndürür.
pub struct ProfileBootstrap {
    pub profiles: Vec<LocalProfile>,
    pub active_profile: Option<LocalProfile>,
    pub should_prompt: bool,
    pub base_dir: PathBuf,
}

/// Home altındaki Amele veri kökünü döndürür.
pub fn amele_home_dir() -> PathBuf {
    settings::home_dir().join("Amele")
}

/// Profil kayıt dosyasının yolunu döndürür.
pub fn profile_store_path() -> PathBuf {
    amele_home_dir().join("profiller.json")
}

/// Profilin güvenli klasör adını üretir.
pub fn profile_slug(username: &str) -> String {
    sanitize_username(username)
}

/// Aktif profilin kullanıcı adını döndürür.
pub fn active_username() -> Option<String> {
    active_profile().map(|profile| profile.username)
}

/// Aktif profilin ayar dosyası yolunu döndürür.
pub fn active_settings_path() -> Option<PathBuf> {
    active_username().map(|username| settings_path_for_username(&username))
}

/// Aktif profilin vaka taban klasörünü döndürür.
pub fn active_case_base_dir() -> Option<PathBuf> {
    active_username().map(|username| case_base_dir_for_username(&username))
}

/// Profil klasörünü döndürür.
pub fn profile_dir_for_username(username: &str) -> PathBuf {
    amele_home_dir()
        .join("Kullanicilar")
        .join(profile_slug(username))
}

/// Profil bazlı ayar dosyasını döndürür.
pub fn settings_path_for_username(username: &str) -> PathBuf {
    profile_dir_for_username(username).join("ayarlar.json")
}

/// Profil bazlı vaka taban klasörünü döndürür.
pub fn case_base_dir_for_username(username: &str) -> PathBuf {
    profile_dir_for_username(username).join("Vakalar")
}

/// Açılışta profil listesini okur ve gerekiyorsa doğrudan açılacak profili aktifler.
pub fn bootstrap_profiles() -> AmeleResult<ProfileBootstrap> {
    let store = load_profile_store()?;
    let direct_profile = store
        .profiles
        .iter()
        .find(|profile| profile.open_directly)
        .cloned();
    let active_profile = direct_profile;

    if let Some(profile) = active_profile.clone() {
        set_active_profile(Some(profile.clone()));
        ensure_profile_dirs(&profile)?;
    } else {
        set_active_profile(None);
    }

    Ok(ProfileBootstrap {
        profiles: store.profiles,
        active_profile: active_profile.clone(),
        should_prompt: active_profile.is_none(),
        base_dir: amele_home_dir(),
    })
}

/// Yeni yerel profil oluşturur ve aktif profil yapar.
pub fn create_profile(
    full_name: &str,
    username: &str,
    language: &str,
    theme: &str,
    open_directly: bool,
) -> AmeleResult<LocalProfile> {
    let username = sanitize_username(username);
    if username.is_empty() {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Kullanıcı adı boş olamaz",
        ));
    }
    let full_name = full_name.trim();
    if full_name.is_empty() {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "İsim soyisim boş olamaz",
        ));
    }

    let mut store = load_profile_store()?;
    if store
        .profiles
        .iter()
        .any(|profile| profile.username == username)
    {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            format!("Bu kullanıcı adı zaten var: {username}"),
        ));
    }

    let now = now_string();
    let profile = LocalProfile {
        username: username.clone(),
        full_name: full_name.to_string(),
        display_name: display_name_from(full_name, &username),
        language: normalize_language(language),
        theme: normalize_theme(theme),
        open_directly,
        created_at: now.clone(),
        last_used_at: now,
    };

    if open_directly {
        for existing in &mut store.profiles {
            existing.open_directly = false;
        }
    }
    store.active_username = Some(username);
    store.profiles.push(profile.clone());
    save_profile_store(&store)?;
    ensure_profile_dirs(&profile)?;
    ensure_profile_settings(&profile)?;
    set_active_profile(Some(profile.clone()));
    Ok(profile)
}

/// Var olan profili seçer ve isteğe göre doğrudan açılacak profil olarak işaretler.
pub fn select_profile(username: &str, open_directly: bool) -> AmeleResult<LocalProfile> {
    let username = sanitize_username(username);
    let mut store = load_profile_store()?;
    let mut selected = None;
    let now = now_string();

    for profile in &mut store.profiles {
        if profile.username == username {
            profile.last_used_at = now.clone();
            profile.open_directly = open_directly;
            selected = Some(profile.clone());
        } else if open_directly {
            profile.open_directly = false;
        }
    }

    let Some(profile) = selected else {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            format!("Profil bulunamadı: {username}"),
        ));
    };

    store.active_username = Some(profile.username.clone());
    save_profile_store(&store)?;
    ensure_profile_dirs(&profile)?;
    ensure_profile_settings(&profile)?;
    set_active_profile(Some(profile.clone()));
    Ok(profile)
}

/// Aktif oturumu kapatır ve sonraki açılışta profil seçimini zorlar.
pub fn logout_profile() -> AmeleResult<()> {
    let mut store = load_profile_store()?;
    store.active_username = None;
    for profile in &mut store.profiles {
        profile.open_directly = false;
    }
    save_profile_store(&store)?;
    set_active_profile(None);
    Ok(())
}

/// Aktif profilin dil ve tema tercihlerini profil deposunda günceller.
pub fn update_active_preferences(language: &str, theme: &str) -> AmeleResult<Option<LocalProfile>> {
    let Some(username) = active_username() else {
        return Ok(None);
    };
    let mut store = load_profile_store()?;
    let mut updated = None;
    for profile in &mut store.profiles {
        if profile.username == username {
            profile.language = normalize_language(language);
            profile.theme = normalize_theme(theme);
            updated = Some(profile.clone());
            break;
        }
    }
    save_profile_store(&store)?;
    if let Some(profile) = updated.clone() {
        set_active_profile(Some(profile));
    }
    Ok(updated)
}

/// Aktif profili döndürür.
pub fn active_profile() -> Option<LocalProfile> {
    active_profile_cell()
        .lock()
        .ok()
        .and_then(|profile| profile.clone())
}

/// Profil deposunu diskten okur.
pub fn load_profile_store() -> AmeleResult<ProfileStore> {
    let path = profile_store_path();
    if !path.is_file() {
        return Ok(ProfileStore::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|err| AmeleError::io(HataKodu::DosyaOkuma, "Profil dosyası okunamadı", err))?;
    serde_json::from_str(&content).map_err(|err| {
        AmeleError::new(
            HataKodu::ProtokolJson,
            format!("Profil dosyası parse edilemedi: {err}"),
        )
    })
}

/// Profil deposunu diske yazar.
pub fn save_profile_store(store: &ProfileStore) -> AmeleResult<()> {
    let path = profile_store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Profil klasörü oluşturulamadı", err)
        })?;
    }
    let content = serde_json::to_string_pretty(store)?;
    fs::write(&path, content)
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Profil dosyası yazılamadı", err))
}

fn ensure_profile_dirs(profile: &LocalProfile) -> AmeleResult<()> {
    for dir in [
        profile_dir_for_username(&profile.username),
        case_base_dir_for_username(&profile.username),
        profile_dir_for_username(&profile.username).join("Ciktilar"),
    ] {
        fs::create_dir_all(&dir).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                format!("Profil klasörü oluşturulamadı: {}", dir.display()),
                err,
            )
        })?;
    }
    Ok(())
}

fn ensure_profile_settings(profile: &LocalProfile) -> AmeleResult<()> {
    let path = settings_path_for_username(&profile.username);
    if path.is_file() {
        return Ok(());
    }
    let mut settings = settings::AppSettings::default();
    let profile_dir = profile_dir_for_username(&profile.username);
    settings.cikti_klasoru = profile_dir.join("Ciktilar");
    settings.vaka_klasoru = profile_dir.join("Vakalar");
    settings.dil = profile.language.clone();
    settings.karanlik_tema = profile.theme != "light";
    settings.save(path)
}

fn set_active_profile(profile: Option<LocalProfile>) {
    if let Ok(mut current) = active_profile_cell().lock() {
        *current = profile;
    }
}

fn active_profile_cell() -> &'static Mutex<Option<LocalProfile>> {
    static ACTIVE_PROFILE: OnceLock<Mutex<Option<LocalProfile>>> = OnceLock::new();
    ACTIVE_PROFILE.get_or_init(|| Mutex::new(None))
}

fn sanitize_username(value: &str) -> String {
    let sanitized: String = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    sanitized.trim_matches('_').to_string()
}

fn display_name_from(full_name: &str, username: &str) -> String {
    full_name
        .split_whitespace()
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(username)
        .to_string()
}

fn normalize_language(value: &str) -> String {
    match value {
        "en" => "en".to_string(),
        _ => "tr".to_string(),
    }
}

fn normalize_theme(value: &str) -> String {
    match value {
        "light" => "light".to_string(),
        _ => "dark".to_string(),
    }
}

fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_slug_is_filesystem_safe() {
        assert_eq!(profile_slug("Melih Emik"), "melih_emik");
        assert_eq!(profile_slug("fav.ilances"), "fav.ilances");
    }
}
