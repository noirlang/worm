//! Yerel profil, profil bazlı ayar ve vaka klasörü yönetimini sağlar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::settings;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<OnlineProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity_log: Vec<ProfileActivityLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// amele.noirlang.tr hesabından senkronlanan profil özetidir.
pub struct OnlineProfile {
    pub user_id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub first_name: String,
    pub last_name: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub has_license: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub licenses: Vec<OnlineLicenseSummary>,
    pub linked_at: String,
    pub last_sync_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worked_case_types: Vec<String>,
    #[serde(default)]
    pub mobile_tools_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Online profil ile ilişkili lisansın UI'ye gösterilecek özetidir.
pub struct OnlineLicenseSummary {
    pub license_key: String,
    pub plan: String,
    pub status: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Profilin hangi mobil/adli vaka türlerinde kullanıldığını yerelde kaydeder.
pub struct ProfileActivityLog {
    pub timestamp: String,
    pub category: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Android/iOS araçlarının online üyelik kilit durumudur.
pub struct MobileToolsAccess {
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<LocalProfile>,
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
        ensure_profile_settings(&profile)?;
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
        online: None,
        activity_log: Vec::new(),
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

/// Online site hesabıyla giriş yapar, yerel profil oluşturur/günceller ve aktifler.
pub fn link_online_profile(
    identifier: &str,
    password: &str,
    language: &str,
    theme: &str,
    open_directly: bool,
) -> AmeleResult<LocalProfile> {
    let auth = online_login(identifier, password)?;
    let online_username = sanitize_username(&auth.user.username);
    if online_username.is_empty() {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Online kullanıcı adı geçersiz",
        ));
    }

    let licenses = match online_fetch_licenses(&auth.token) {
        Ok(licenses) => licenses,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Warn,
                "profile:online",
                format!("Online lisanslar okunamadı: {err}"),
            );
            Vec::new()
        }
    };

    let now = now_string();
    let mut store = load_profile_store()?;
    let local_username = active_username()
        .filter(|username| {
            store
                .profiles
                .iter()
                .any(|profile| profile.username == *username)
        })
        .unwrap_or_else(|| online_username.clone());
    let existing_index = store
        .profiles
        .iter()
        .position(|profile| profile.username == local_username);
    let existing_activity = existing_index
        .and_then(|index| store.profiles.get(index))
        .map(|profile| profile.activity_log.clone())
        .unwrap_or_default();
    let linked_at = existing_index
        .and_then(|index| store.profiles.get(index))
        .and_then(|profile| profile.online.as_ref())
        .map(|online| online.linked_at.clone())
        .unwrap_or_else(|| now.clone());
    let worked_case_types = worked_case_types_from_log(&existing_activity);
    let online = online_profile_from_user(
        auth.user,
        licenses,
        linked_at,
        now.clone(),
        worked_case_types,
    );
    let full_name = online_full_name(&online);

    if open_directly {
        for existing in &mut store.profiles {
            existing.open_directly = false;
        }
    }

    let profile = if let Some(index) = existing_index {
        let profile = &mut store.profiles[index];
        profile.language = normalize_language(language);
        profile.theme = normalize_theme(theme);
        profile.open_directly = open_directly;
        profile.last_used_at = now.clone();
        profile.online = Some(online);
        profile.clone()
    } else {
        let profile = LocalProfile {
            username: local_username.clone(),
            full_name,
            display_name: display_name_from(&online_full_name(&online), &local_username),
            language: normalize_language(language),
            theme: normalize_theme(theme),
            open_directly,
            created_at: now.clone(),
            last_used_at: now.clone(),
            online: Some(online),
            activity_log: Vec::new(),
        };
        store.profiles.push(profile.clone());
        profile
    };

    ensure_profile_dirs(&profile)?;
    ensure_profile_settings(&profile)?;
    save_online_token(&profile.username, &auth.token)?;
    store.active_username = Some(profile.username.clone());
    save_profile_store(&store)?;
    set_active_profile(Some(profile.clone()));
    Ok(profile)
}

/// Aktif yerel profilin online bilgilerini site API'sinden yeniler.
pub fn sync_active_online_profile() -> AmeleResult<LocalProfile> {
    let Some(current) = active_profile() else {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Aktif profil yok",
        ));
    };
    let Some(current_online) = current.online else {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            mobile_tools_required_message(),
        ));
    };
    let token = load_online_token(&current.username)?;
    let session_user = online_fetch_session(&token)?;
    ensure_session_matches_profile(&session_user, &current_online)?;
    let user = online_fetch_user(&token, &current_online.username)?;
    let licenses = match online_fetch_licenses(&token) {
        Ok(licenses) => licenses,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Warn,
                "profile:online",
                format!("Online lisanslar senkronlanamadı: {err}"),
            );
            Vec::new()
        }
    };

    let mut store = load_profile_store()?;
    let mut updated = None;
    let now = now_string();
    for profile in &mut store.profiles {
        if profile.username == current.username {
            let worked_case_types = worked_case_types_from_log(&profile.activity_log);
            let online = online_profile_from_user(
                user,
                licenses,
                current_online.linked_at,
                now,
                worked_case_types,
            );
            profile.online = Some(online);
            updated = Some(profile.clone());
            break;
        }
    }

    let Some(profile) = updated else {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Aktif profil depoda bulunamadı",
        ));
    };
    save_profile_store(&store)?;
    set_active_profile(Some(profile.clone()));
    Ok(profile)
}

/// Aktif yerel profilden online hesap bağlantısını kaldırır.
pub fn disconnect_active_online_profile() -> AmeleResult<Option<LocalProfile>> {
    let Some(username) = active_username() else {
        return Ok(None);
    };
    let mut store = load_profile_store()?;
    let mut updated = None;
    for profile in &mut store.profiles {
        if profile.username == username {
            profile.online = None;
            updated = Some(profile.clone());
            break;
        }
    }
    remove_online_token(&username)?;
    save_profile_store(&store)?;
    set_active_profile(updated.clone());
    Ok(updated)
}

/// Aktif profil için mobil araç erişim durumunu döndürür.
pub fn mobile_tools_access() -> MobileToolsAccess {
    let profile = active_profile();
    let allowed = profile
        .as_ref()
        .and_then(|profile| profile.online.as_ref())
        .is_some_and(online_profile_unlocks_mobile_tools);

    MobileToolsAccess {
        allowed,
        reason: (!allowed).then(|| mobile_tools_required_message().to_string()),
        profile,
    }
}

/// Android/iOS araçları için backend tarafında da üyelik zorunluluğu sağlar.
pub fn require_mobile_tools_access() -> AmeleResult<()> {
    let Some(profile) = active_profile() else {
        return Err(AmeleError::new(
            HataKodu::YetkisizErisim,
            mobile_tools_required_message(),
        ));
    };
    let Some(online) = profile.online.as_ref() else {
        return Err(AmeleError::new(
            HataKodu::YetkisizErisim,
            mobile_tools_required_message(),
        ));
    };
    let token = load_online_token(&profile.username)
        .map_err(|_| AmeleError::new(HataKodu::YetkisizErisim, mobile_tools_required_message()))?;
    let session_user = online_fetch_session(&token)?;
    ensure_session_matches_profile(&session_user, online)?;
    let current_user = online_fetch_user(&token, &online.username)?;
    ensure_session_matches_profile(&current_user, online)?;
    if online_user_unlocks_mobile_tools(&current_user) {
        return Ok(());
    }
    Err(AmeleError::new(
        HataKodu::YetkisizErisim,
        "Bu online hesabın Android/iOS araçları için gerekli rolü yok.",
    ))
}

/// Profil aktivitesini yerel profile kaydeder ve online çalışma türü özetini günceller.
pub fn record_active_profile_activity(
    category: &str,
    action: &str,
    case_name: Option<&str>,
    details: Option<&str>,
) -> AmeleResult<Option<LocalProfile>> {
    let Some(username) = active_username() else {
        return Ok(None);
    };
    let category = category.trim();
    let action = action.trim();
    if category.is_empty() || action.is_empty() {
        return Ok(active_profile());
    }

    let mut store = load_profile_store()?;
    let mut updated = None;
    for profile in &mut store.profiles {
        if profile.username == username {
            profile.activity_log.push(ProfileActivityLog {
                timestamp: now_string(),
                category: category.to_string(),
                action: action.to_string(),
                case_name: case_name
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
                details: details
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
            });
            if profile.activity_log.len() > 200 {
                let extra = profile.activity_log.len() - 200;
                profile.activity_log.drain(0..extra);
            }
            let worked_case_types = worked_case_types_from_log(&profile.activity_log);
            if let Some(online) = &mut profile.online {
                online.worked_case_types = worked_case_types;
            }
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
    let mut settings = if path.is_file() {
        settings::AppSettings::load(&path)?
    } else {
        settings::AppSettings::default()
    };
    let profile_dir = profile_dir_for_username(&profile.username);
    settings.cikti_klasoru = profile_dir.join("Ciktilar");
    settings.vaka_klasoru = profile_dir.join("Vakalar");
    settings.dil = profile.language.clone();
    settings.karanlik_tema = profile.theme != "light";
    settings.normalize();
    settings.save(path)
}

fn online_token_path(username: &str) -> PathBuf {
    profile_dir_for_username(username).join("online_token")
}

fn save_online_token(username: &str, token: &str) -> AmeleResult<()> {
    let path = online_token_path(username);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                "Online profil klasörü oluşturulamadı",
                err,
            )
        })?;
    }
    fs::write(&path, token.trim()).map_err(|err| {
        AmeleError::io(HataKodu::DosyaYazma, "Online token dosyası yazılamadı", err)
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn load_online_token(username: &str) -> AmeleResult<String> {
    let path = online_token_path(username);
    let token = fs::read_to_string(&path).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaOkuma,
            "Online profil token dosyası okunamadı. Yeniden giriş yapın.",
            err,
        )
    })?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Online profil token dosyası boş. Yeniden giriş yapın.",
        ));
    }
    Ok(token)
}

fn remove_online_token(username: &str) -> AmeleResult<()> {
    let path = online_token_path(username);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AmeleError::io(
            HataKodu::DosyaYazma,
            "Online token dosyası silinemedi",
            err,
        )),
    }
}

fn online_api_base_url() -> String {
    std::env::var("AMELE_ONLINE_API_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://amele.noirlang.tr".to_string())
}

fn online_origin() -> String {
    origin_from_url(&online_api_base_url())
        .unwrap_or_else(|| "https://amele.noirlang.tr".to_string())
}

fn origin_from_url(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let rest = &url[scheme_end + 3..];
    let host = rest.split('/').next()?.trim();
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

const COOKIE_CREDENTIAL_PREFIX: &str = "cookie:";

#[derive(Debug)]
struct OnlineAuthResponse {
    user: OnlineUserResponse,
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineUserResponse {
    #[serde(alias = "_id")]
    id: String,
    username: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    has_license: bool,
}

#[derive(Debug, Deserialize)]
struct OnlineLicensesEnvelope {
    #[serde(default)]
    licenses: Vec<OnlineLicenseResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineLicenseResponse {
    #[serde(default)]
    license_key: String,
    #[serde(default)]
    plan: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    created_at: Value,
    #[serde(default)]
    expires_at: Option<Value>,
}

fn online_login(identifier: &str, password: &str) -> AmeleResult<OnlineAuthResponse> {
    let payload = serde_json::json!({
        "identifier": identifier.trim(),
        "password": password,
    });
    let url = format!("{}/api/auth/login", online_api_base_url());
    let response = online_post_json_with_session_cookie(&url, None, &payload)?;
    parse_online_auth_response(response.value, response.session_cookie)
}

fn online_fetch_user(token: &str, username: &str) -> AmeleResult<OnlineUserResponse> {
    let url = format!("{}/api/users/{}", online_api_base_url(), username);
    let value = online_get_json(&url, Some(token))?;
    parse_required_online_user(&value, "Online kullanıcı cevabı parse edilemedi")
}

fn online_fetch_session(token: &str) -> AmeleResult<OnlineUserResponse> {
    let url = format!("{}/api/auth/session", online_api_base_url());
    let value = online_get_json(&url, Some(token))?;
    parse_optional_online_user(&value, "Online oturum cevabı parse edilemedi")?.ok_or_else(|| {
        AmeleError::new(
            HataKodu::YetkisizErisim,
            "Online oturum geçersiz. Yeniden giriş yapın.",
        )
    })
}

fn online_fetch_licenses(token: &str) -> AmeleResult<Vec<OnlineLicenseSummary>> {
    let url = format!("{}/api/licenses/my", online_api_base_url());
    let value = online_get_json(&url, Some(token))?;
    let envelope: OnlineLicensesEnvelope = serde_json::from_value(response_payload(&value).clone())
        .map_err(|err| {
            AmeleError::new(
                HataKodu::ProtokolJson,
                format!("Online lisans cevabı parse edilemedi: {err}"),
            )
        })?;
    Ok(envelope
        .licenses
        .into_iter()
        .map(|license| OnlineLicenseSummary {
            license_key: mask_secret(&license.license_key),
            plan: license.plan,
            status: license.status,
            created_at: value_to_display_string(&license.created_at),
            expires_at: license.expires_at.as_ref().map(value_to_display_string),
        })
        .collect())
}

fn online_get_json(url: &str, bearer: Option<&str>) -> AmeleResult<Value> {
    let agent = online_agent();
    let request = apply_online_headers(agent.get(url), bearer);
    let response = request
        .call()
        .map_err(|err| online_request_error("Online API isteği başarısız", err))?;
    parse_online_response(response).map(|response| response.value)
}

fn online_post_json_with_session_cookie(
    url: &str,
    bearer: Option<&str>,
    payload: &Value,
) -> AmeleResult<OnlineJsonResponse> {
    let agent = online_agent();
    let request =
        apply_online_headers(agent.post(url), bearer).set("Content-Type", "application/json");
    let response = request
        .send_string(&payload.to_string())
        .map_err(|err| online_request_error("Online API isteği başarısız", err))?;
    parse_online_response(response)
}

#[derive(Debug)]
struct OnlineJsonResponse {
    value: Value,
    session_cookie: Option<String>,
}

fn online_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build()
}

fn apply_online_headers(request: ureq::Request, bearer: Option<&str>) -> ureq::Request {
    let origin = online_origin();
    let referer = format!("{origin}/");
    let request = request
        .set("Accept", "application/json")
        .set("Origin", &origin)
        .set("Referer", &referer)
        .set("User-Agent", concat!("Amele/", env!("CARGO_PKG_VERSION")));
    if let Some(credential) = bearer {
        if let Some(cookie) = credential.strip_prefix(COOKIE_CREDENTIAL_PREFIX) {
            request.set("Cookie", cookie)
        } else {
            request.set("Authorization", &format!("Bearer {credential}"))
        }
    } else {
        request
    }
}

fn parse_online_response(response: ureq::Response) -> AmeleResult<OnlineJsonResponse> {
    let session_cookie = response
        .header("Set-Cookie")
        .and_then(session_cookie_from_header)
        .map(cookie_credential);
    let text = response.into_string().map_err(|err| {
        AmeleError::new(
            HataKodu::DosyaOkuma,
            format!("Online API cevabı okunamadı: {err}"),
        )
    })?;
    let value = serde_json::from_str(&text).map_err(|err| {
        AmeleError::new(
            HataKodu::ProtokolJson,
            format!("Online API cevabı JSON değil: {err}"),
        )
    })?;
    Ok(OnlineJsonResponse {
        value,
        session_cookie,
    })
}

fn parse_online_auth_response(
    value: Value,
    session_cookie: Option<String>,
) -> AmeleResult<OnlineAuthResponse> {
    let user = parse_required_online_user(&value, "Online giriş cevabı parse edilemedi")?;
    let payload = response_payload(&value);
    let token = string_field(payload, &["token", "accessToken", "jwt"])
        .or_else(|| string_field(&value, &["token", "accessToken", "jwt"]))
        .map(ToOwned::to_owned)
        .or(session_cookie)
        .ok_or_else(|| {
            AmeleError::new(
                HataKodu::ProtokolJson,
                format!(
                    "Online giriş cevabında token veya session cookie yok. Gelen alanlar: {}",
                    json_shape(&value)
                ),
            )
        })?;

    Ok(OnlineAuthResponse { user, token })
}

fn parse_required_online_user(value: &Value, context: &str) -> AmeleResult<OnlineUserResponse> {
    parse_optional_online_user(value, context)?.ok_or_else(|| {
        AmeleError::new(
            HataKodu::ProtokolJson,
            format!(
                "{context}: user alanı yok. Gelen alanlar: {}",
                json_shape(value)
            ),
        )
    })
}

fn parse_optional_online_user(
    value: &Value,
    context: &str,
) -> AmeleResult<Option<OnlineUserResponse>> {
    let payload = response_payload(value);
    let Some(user_value) = payload.get("user").or_else(|| value.get("user")) else {
        return Ok(None);
    };
    if user_value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(user_value.clone())
        .map(Some)
        .map_err(|err| AmeleError::new(HataKodu::ProtokolJson, format!("{context}: {err}")))
}

fn response_payload(value: &Value) -> &Value {
    value
        .get("data")
        .filter(|payload| payload.is_object())
        .unwrap_or(value)
}

fn string_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn session_cookie_from_header(header: &str) -> Option<String> {
    ["amele_token", "amele_session"]
        .iter()
        .find_map(|name| extract_cookie(header, name))
}

fn extract_cookie(header: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = header.find(&needle)?;
    let rest = &header[start..];
    let end = rest.find(';').unwrap_or(rest.len());
    let cookie = rest[..end].trim();
    if cookie.is_empty() || cookie.contains('\r') || cookie.contains('\n') {
        None
    } else {
        Some(cookie.to_string())
    }
}

fn cookie_credential(cookie: String) -> String {
    format!("{COOKIE_CREDENTIAL_PREFIX}{cookie}")
}

fn json_shape(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let keys = map.keys().map(String::as_str).collect::<Vec<_>>();
            format!("{{{}}}", keys.join(", "))
        }
        Value::Array(_) => "array".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::Null => "null".to_string(),
    }
}

fn online_request_error(context: &str, err: ureq::Error) -> AmeleError {
    match err {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            let code = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("code")
                        .and_then(Value::as_str)
                        .or_else(|| value.get("message").and_then(Value::as_str))
                        .map(ToOwned::to_owned)
                })
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| body.trim().to_string());
            AmeleError::new(
                HataKodu::Baglanti,
                format!("{context}: HTTP {status} {}", code.trim()),
            )
        }
        ureq::Error::Transport(err) => {
            AmeleError::new(HataKodu::Baglanti, format!("{context}: {err}"))
        }
    }
}

fn online_profile_from_user(
    user: OnlineUserResponse,
    licenses: Vec<OnlineLicenseSummary>,
    linked_at: String,
    last_sync_at: String,
    worked_case_types: Vec<String>,
) -> OnlineProfile {
    let roles = if user.roles.is_empty() {
        vec!["member".to_string()]
    } else {
        user.roles
    };
    let has_active_license = licenses.iter().any(|license| license.status == "active");
    let mobile_tools_enabled = !roles.is_empty() || user.has_license || has_active_license;
    OnlineProfile {
        user_id: user.id,
        username: user.username,
        email: user.email,
        first_name: user.first_name,
        last_name: user.last_name,
        roles,
        has_license: user.has_license || has_active_license,
        licenses,
        linked_at,
        last_sync_at,
        worked_case_types,
        mobile_tools_enabled,
    }
}

fn online_full_name(online: &OnlineProfile) -> String {
    let full_name = [online.first_name.as_str(), online.last_name.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if full_name.trim().is_empty() {
        online.username.clone()
    } else {
        full_name
    }
}

fn worked_case_types_from_log(log: &[ProfileActivityLog]) -> Vec<String> {
    let mut values = Vec::new();
    for entry in log {
        let category = entry.category.trim();
        if category.is_empty() || values.iter().any(|value| value == category) {
            continue;
        }
        values.push(category.to_string());
    }
    values
}

fn online_profile_unlocks_mobile_tools(online: &OnlineProfile) -> bool {
    online.mobile_tools_enabled
        && online.roles.iter().any(|role| {
            role == "member"
                || role.ends_with("maintainer")
                || role == "developer"
                || role == "bdfl"
        })
}

fn online_user_unlocks_mobile_tools(user: &OnlineUserResponse) -> bool {
    user.roles.iter().any(|role| {
        role == "member" || role.ends_with("maintainer") || role == "developer" || role == "bdfl"
    })
}

fn ensure_session_matches_profile(
    user: &OnlineUserResponse,
    online: &OnlineProfile,
) -> AmeleResult<()> {
    if user.id != online.user_id || user.username != online.username {
        return Err(AmeleError::new(
            HataKodu::YetkisizErisim,
            "Online oturum bağlı profille eşleşmiyor. Yeniden giriş yapın.",
        ));
    }
    Ok(())
}

fn mobile_tools_required_message() -> &'static str {
    "Android ve iOS araçlarını kullanmak için online üyelikle giriş yapın."
}

fn value_to_display_string(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        value.to_string()
    } else if value.is_null() {
        String::new()
    } else {
        value.to_string()
    }
}

fn mask_secret(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let visible = value.chars().rev().take(4).collect::<Vec<_>>();
    let suffix = visible.into_iter().rev().collect::<String>();
    format!("****{suffix}")
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

    #[test]
    fn legacy_profile_json_deserializes_without_online_fields() {
        let profile: LocalProfile = serde_json::from_str(
            r#"{
              "username":"melih",
              "full_name":"Melih Emik",
              "display_name":"Melih",
              "language":"tr",
              "theme":"dark",
              "open_directly":false,
              "created_at":"2026-01-01 00:00:00",
              "last_used_at":"2026-01-01 00:00:00"
            }"#,
        )
        .unwrap();

        assert!(profile.online.is_none());
        assert!(profile.activity_log.is_empty());
    }

    #[test]
    fn online_member_role_unlocks_mobile_tools() {
        let online = OnlineProfile {
            user_id: "1".to_string(),
            username: "melih".to_string(),
            email: None,
            first_name: "Melih".to_string(),
            last_name: "Emik".to_string(),
            roles: vec!["member".to_string()],
            has_license: false,
            licenses: Vec::new(),
            linked_at: "2026-01-01 00:00:00".to_string(),
            last_sync_at: "2026-01-01 00:00:00".to_string(),
            worked_case_types: Vec::new(),
            mobile_tools_enabled: true,
        };

        assert!(online_profile_unlocks_mobile_tools(&online));
    }

    #[test]
    fn derives_origin_from_online_api_base_url() {
        assert_eq!(
            origin_from_url("https://amele.noirlang.tr/api").as_deref(),
            Some("https://amele.noirlang.tr")
        );
        assert_eq!(
            origin_from_url("http://localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert!(origin_from_url("file:///tmp/api").is_none());
    }

    #[test]
    fn masks_license_keys_before_profile_storage() {
        assert_eq!(mask_secret("AMELE-1234-5678"), "****5678");
        assert_eq!(mask_secret(""), "");
    }

    #[test]
    fn parses_login_token_from_json_response() {
        let auth = parse_online_auth_response(
            serde_json::json!({
                "user": {
                    "id": "u1",
                    "username": "melih",
                    "firstName": "Melih",
                    "lastName": "Emik",
                    "roles": ["bdfl"],
                    "hasLicense": true
                },
                "token": "jwt-token"
            }),
            None,
        )
        .unwrap();

        assert_eq!(auth.user.username, "melih");
        assert_eq!(auth.token, "jwt-token");
    }

    #[test]
    fn parses_login_session_cookie_when_token_is_absent() {
        let auth = parse_online_auth_response(
            serde_json::json!({
                "data": {
                    "user": {
                        "id": "u1",
                        "username": "melih",
                        "firstName": "Melih",
                        "lastName": "Emik",
                        "roles": ["member"]
                    }
                }
            }),
            session_cookie_from_header(
                "amele_session=abc.def; Path=/; HttpOnly; Secure; SameSite=Lax",
            )
            .map(cookie_credential),
        )
        .unwrap();

        assert_eq!(auth.user.username, "melih");
        assert_eq!(auth.token, "cookie:amele_session=abc.def");
    }
}
