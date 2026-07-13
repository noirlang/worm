//! Uygulama güncelleme kontrolü, indirme ve paket kurulumu API uçlarını içerir.
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::server::{Response, json_error, json_ok};

use super::{home_dir, process_is_root, sha256_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdatePackageKind {
    WindowsMsi,
    AppImage,
    Deb,
    Rpm,
    Pacman,
    Tarball,
}

#[derive(Debug, Clone)]
struct UpdateTarget {
    kind: UpdatePackageKind,
    detected_by: &'static str,
}

impl UpdatePackageKind {
    fn id(self) -> &'static str {
        match self {
            Self::WindowsMsi => "windows_msi",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Pacman => "pacman",
            Self::Tarball => "tarball",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::WindowsMsi => "Windows MSI",
            Self::AppImage => "Linux AppImage",
            Self::Deb => "Debian/Ubuntu .deb",
            Self::Rpm => "Fedora/RHEL .rpm",
            Self::Pacman => "Arch Linux .pkg.tar.zst",
            Self::Tarball => "Linux tarball",
        }
    }

    fn from_asset_name(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        if name.ends_with(".msi") {
            Some(Self::WindowsMsi)
        } else if name.ends_with(".appimage") {
            Some(Self::AppImage)
        } else if name.ends_with(".deb") {
            Some(Self::Deb)
        } else if name.ends_with(".rpm") {
            Some(Self::Rpm)
        } else if name.contains(".pkg.tar.") || is_legacy_arch_asset_name(&name) {
            Some(Self::Pacman)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(Self::Tarball)
        } else {
            None
        }
    }
}

fn is_legacy_arch_asset_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "amele-linux-x64.tar.zst" || name == "amele-forensic-tool-linux-x64.tar.zst"
}

/// GitHub release API üzerinden güncelleme bilgisi alır.
pub fn update_check_endpoint() -> Response {
    let output = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("https://api.github.com/repos/noirlang/amele/releases/latest")
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return json_error(
                500,
                if stderr.is_empty() {
                    "release check failed".to_string()
                } else {
                    stderr
                },
            );
        }
        Err(err) => return json_error(500, err.to_string()),
    };

    let release: Value = match serde_json::from_slice(&output.stdout) {
        Ok(release) => release,
        Err(err) => return json_error(500, err.to_string()),
    };
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .map(|assets| {
            assets
                .iter()
                .map(|asset| {
                    json!({
                        "name": asset.get("name").and_then(Value::as_str).unwrap_or_default(),
                        "download_url": asset.get("browser_download_url").and_then(Value::as_str).unwrap_or_default(),
                        "size": asset.get("size").and_then(Value::as_u64).unwrap_or_default(),
                        "digest": asset.get("digest").and_then(Value::as_str).unwrap_or_default(),
                    })
                })
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default();
    let update_target = current_update_target();
    let platform_asset = preferred_update_asset(&assets, &update_target);
    let asset_error = if platform_asset.is_null() {
        Some(missing_platform_asset_message(&update_target, &assets))
    } else {
        None
    };

    json_ok(json!({
        "current_version": env!("CARGO_PKG_VERSION"),
        "tag_name": release.get("tag_name").and_then(Value::as_str).unwrap_or_default(),
        "name": release.get("name").and_then(Value::as_str).unwrap_or_default(),
        "html_url": release.get("html_url").and_then(Value::as_str).unwrap_or_default(),
        "body": release.get("body").and_then(Value::as_str).unwrap_or_default(),
        "assets": assets,
        "update_target": update_target_json(&update_target, platform_asset.get("name").and_then(Value::as_str)),
        "platform_asset": platform_asset,
        "asset_error": asset_error,
    }))
}

/// Ag baglantisi gerektirmeden mevcut sistemde hangi paket tipinin kullanildigini doner.
pub fn update_target_endpoint() -> Response {
    let update_target = current_update_target();
    json_ok(update_target_json(&update_target, None))
}

/// Seçilen release asset'ini indirir ve hash doğrulaması yapar.
pub fn update_download_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct UpdateDownloadRequest {
        url: String,
        name: Option<String>,
        output_dir: Option<String>,
        expected_sha256: Option<String>,
    }

    let request: UpdateDownloadRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let url = request.url.trim();
    if url.is_empty() {
        return json_error(400, "url is required");
    }
    let output_dir = request
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);
    if let Err(err) = fs::create_dir_all(&output_dir) {
        return json_error(500, err.to_string());
    }
    let name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(sanitize_download_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "amele-update.bin".to_string());
    let target = output_dir.join(name);
    let output = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("-o")
        .arg(&target)
        .arg(url)
        .output();

    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = fs::remove_file(&target);
            return json_error(
                500,
                if stderr.is_empty() {
                    "download failed".to_string()
                } else {
                    stderr
                },
            );
        }
        Err(err) => return json_error(500, err.to_string()),
    }

    let sha256 = match sha256_file(&target) {
        Ok(value) => value,
        Err(err) => return json_error(500, err),
    };
    if let Some(expected) = request.expected_sha256 {
        let expected = expected
            .trim()
            .strip_prefix("sha256:")
            .unwrap_or_else(|| expected.trim())
            .to_ascii_lowercase();
        if !expected.is_empty() && expected != sha256 {
            let _ = fs::remove_file(&target);
            return json_error(500, "downloaded file sha256 mismatch");
        }
    }
    let size = fs::metadata(&target)
        .map(|meta| meta.len())
        .unwrap_or_default();

    json_ok(json!({
        "path": target,
        "size": size,
        "sha256": sha256,
    }))
}

/// İndirilmiş güncelleme paketini platforma göre çalıştırır.
pub fn update_install_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct UpdateInstallRequest {
        path: String,
    }

    let request: UpdateInstallRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = PathBuf::from(request.path.trim());
    if path.as_os_str().is_empty() {
        return json_error(400, "path is required");
    }
    if !path.is_file() {
        return json_error(404, "update package not found");
    }

    match launch_update_installer(&path) {
        Ok(message) => json_ok(json!({ "path": path, "message": message })),
        Err(err) => json_error(500, err),
    }
}

fn preferred_update_asset(assets: &[Value], target: &UpdateTarget) -> Value {
    let mut asset = preferred_update_asset_for_kind(assets, target.kind)
        .or_else(|| {
            if matches!(
                target.kind,
                UpdatePackageKind::AppImage | UpdatePackageKind::Tarball
            ) || target.detected_by == "fallback"
            {
                preferred_update_asset_fallback(assets)
            } else {
                None
            }
        })
        .unwrap_or(Value::Null);

    if let Some(object) = asset.as_object_mut() {
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let kind = UpdatePackageKind::from_asset_name(&name).unwrap_or(target.kind);
        object.insert("package_kind".to_string(), json!(kind.id()));
        object.insert("package_label".to_string(), json!(kind.label()));
        object.insert(
            "install_command".to_string(),
            json!(update_install_preview(kind, Some(&name))),
        );
    }

    asset
}

fn preferred_update_asset_for_kind(assets: &[Value], kind: UpdatePackageKind) -> Option<Value> {
    assets
        .iter()
        .find(|asset| {
            let name = asset
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            UpdatePackageKind::from_asset_name(name) == Some(kind)
        })
        .cloned()
}

fn preferred_update_asset_fallback(assets: &[Value]) -> Option<Value> {
    let fallback_order = if cfg!(target_os = "windows") {
        [
            UpdatePackageKind::WindowsMsi,
            UpdatePackageKind::AppImage,
            UpdatePackageKind::Deb,
            UpdatePackageKind::Rpm,
            UpdatePackageKind::Pacman,
            UpdatePackageKind::Tarball,
        ]
    } else {
        [
            UpdatePackageKind::AppImage,
            UpdatePackageKind::Deb,
            UpdatePackageKind::Rpm,
            UpdatePackageKind::Pacman,
            UpdatePackageKind::Tarball,
            UpdatePackageKind::WindowsMsi,
        ]
    };

    fallback_order
        .iter()
        .find_map(|kind| preferred_update_asset_for_kind(assets, *kind))
        .or_else(|| assets.first().cloned())
}

fn current_update_target() -> UpdateTarget {
    #[cfg(target_os = "windows")]
    {
        return UpdateTarget {
            kind: UpdatePackageKind::WindowsMsi,
            detected_by: "windows",
        };
    }

    #[cfg(target_os = "linux")]
    {
        return detect_linux_update_target();
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        UpdateTarget {
            kind: UpdatePackageKind::Tarball,
            detected_by: "generic",
        }
    }
}

fn update_target_json(target: &UpdateTarget, asset_name: Option<&str>) -> Value {
    let asset_kind = asset_name
        .and_then(UpdatePackageKind::from_asset_name)
        .unwrap_or(target.kind);
    json!({
        "platform": if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "linux") { "linux" } else { "other" },
        "package_kind": target.kind.id(),
        "package_label": target.kind.label(),
        "asset_package_kind": asset_kind.id(),
        "asset_package_label": asset_kind.label(),
        "detected_by": target.detected_by,
        "install_command": update_install_preview(asset_kind, asset_name),
    })
}

fn missing_platform_asset_message(target: &UpdateTarget, assets: &[Value]) -> String {
    let available = assets
        .iter()
        .filter_map(|asset| asset.get("name").and_then(Value::as_str))
        .filter(|name| !name.is_empty())
        .collect::<Vec<&str>>();
    let expected = match target.kind {
        UpdatePackageKind::WindowsMsi => ".msi",
        UpdatePackageKind::AppImage => ".AppImage",
        UpdatePackageKind::Deb => ".deb",
        UpdatePackageKind::Rpm => ".rpm",
        UpdatePackageKind::Pacman => ".pkg.tar.zst",
        UpdatePackageKind::Tarball => ".tar.gz",
    };
    format!(
        "{} paketi release içinde bulunamadı. Beklenen asset: {}. Algılama: {}. Release assetleri: {}",
        target.kind.label(),
        expected,
        target.detected_by,
        if available.is_empty() {
            "(asset yok)".to_string()
        } else {
            available.join(", ")
        }
    )
}

#[cfg(target_os = "linux")]
fn detect_linux_update_target() -> UpdateTarget {
    if let Ok(value) = std::env::var("AMELE_UPDATE_PACKAGE_KIND")
        && let Some(kind) = parse_linux_update_kind(&value)
    {
        return UpdateTarget {
            kind,
            detected_by: "env",
        };
    }

    if std::env::var_os("APPIMAGE").is_some()
        || std::env::current_exe()
            .ok()
            .and_then(|path| path.to_str().map(|value| value.to_ascii_lowercase()))
            .map(|path| path.ends_with(".appimage"))
            .unwrap_or(false)
    {
        return UpdateTarget {
            kind: UpdatePackageKind::AppImage,
            detected_by: "appimage",
        };
    }

    if let Some(target) = detect_linux_kind_from_current_exe() {
        return target;
    }

    if package_query_succeeds("pacman", &["-Q"]) {
        return UpdateTarget {
            kind: UpdatePackageKind::Pacman,
            detected_by: "pacman-db",
        };
    }
    if package_query_succeeds("dpkg-query", &["-W"]) {
        return UpdateTarget {
            kind: UpdatePackageKind::Deb,
            detected_by: "dpkg-db",
        };
    }
    if package_query_succeeds("rpm", &["-q"]) {
        return UpdateTarget {
            kind: UpdatePackageKind::Rpm,
            detected_by: "rpm-db",
        };
    }

    if let Some(kind) = detect_linux_kind_from_os_release() {
        return UpdateTarget {
            kind,
            detected_by: "os-release",
        };
    }

    if command_exists("pacman") {
        return UpdateTarget {
            kind: UpdatePackageKind::Pacman,
            detected_by: "pacman",
        };
    }
    if command_exists("apt") || command_exists("dpkg") {
        return UpdateTarget {
            kind: UpdatePackageKind::Deb,
            detected_by: "apt-dpkg",
        };
    }
    if command_exists("dnf")
        || command_exists("yum")
        || command_exists("zypper")
        || command_exists("rpm")
    {
        return UpdateTarget {
            kind: UpdatePackageKind::Rpm,
            detected_by: "rpm-tool",
        };
    }

    UpdateTarget {
        kind: UpdatePackageKind::AppImage,
        detected_by: "fallback",
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_kind_from_current_exe() -> Option<UpdateTarget> {
    let exe = std::env::current_exe().ok()?;
    if package_file_query_succeeds("pacman", &["-Qo"], &exe) {
        return Some(UpdateTarget {
            kind: UpdatePackageKind::Pacman,
            detected_by: "pacman-owner",
        });
    }
    if package_file_query_succeeds("dpkg-query", &["-S"], &exe) {
        return Some(UpdateTarget {
            kind: UpdatePackageKind::Deb,
            detected_by: "dpkg-owner",
        });
    }
    if package_file_query_succeeds("rpm", &["-qf"], &exe) {
        return Some(UpdateTarget {
            kind: UpdatePackageKind::Rpm,
            detected_by: "rpm-owner",
        });
    }
    None
}

#[cfg(target_os = "linux")]
fn package_file_query_succeeds(program: &str, args: &[&str], path: &Path) -> bool {
    Command::new(program)
        .args(args)
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn parse_linux_update_kind(value: &str) -> Option<UpdatePackageKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "appimage" => Some(UpdatePackageKind::AppImage),
        "deb" | "debian" | "ubuntu" => Some(UpdatePackageKind::Deb),
        "rpm" | "fedora" | "rhel" | "opensuse" => Some(UpdatePackageKind::Rpm),
        "pacman" | "arch" | "pkg.tar.zst" => Some(UpdatePackageKind::Pacman),
        "tarball" | "tar.gz" => Some(UpdatePackageKind::Tarball),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn package_query_succeeds(program: &str, args: &[&str]) -> bool {
    const PACKAGE_NAMES: &[&str] = &["amele-forensic-tool", "amele"];

    PACKAGE_NAMES.iter().any(|name| {
        Command::new(program)
            .args(args)
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

#[cfg(target_os = "linux")]
fn detect_linux_kind_from_os_release() -> Option<UpdatePackageKind> {
    let content = fs::read_to_string("/etc/os-release")
        .ok()?
        .to_ascii_lowercase();
    if content.contains("id=arch")
        || content.contains("id=manjaro")
        || content.contains("id_like=arch")
    {
        Some(UpdatePackageKind::Pacman)
    } else if content.contains("id=debian")
        || content.contains("id=ubuntu")
        || content.contains("id_like=debian")
    {
        Some(UpdatePackageKind::Deb)
    } else if content.contains("id=fedora")
        || content.contains("id=rhel")
        || content.contains("id=centos")
        || content.contains("id=opensuse")
        || content.contains("id_like=fedora")
        || content.contains("id_like=\"rhel fedora\"")
        || content.contains("id_like=\"suse\"")
        || content.contains("id_like=suse")
    {
        Some(UpdatePackageKind::Rpm)
    } else {
        None
    }
}

fn update_install_preview(kind: UpdatePackageKind, asset_name: Option<&str>) -> String {
    let name = asset_name.unwrap_or("amele-update-package");
    match kind {
        UpdatePackageKind::WindowsMsi => format!("msiexec /i {}", shell_quote(name)),
        UpdatePackageKind::AppImage => {
            format!("chmod +x {} && {}", shell_quote(name), shell_quote(name))
        }
        UpdatePackageKind::Deb => format!("sudo apt install {}", shell_quote(name)),
        UpdatePackageKind::Rpm => format!("sudo dnf install {}", shell_quote(name)),
        UpdatePackageKind::Pacman => format!("sudo pacman -U {}", shell_quote(name)),
        UpdatePackageKind::Tarball => format!("tar -xzf {}", shell_quote(name)),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|err| err.to_string())
}

#[cfg(unix)]
fn linux_package_install_command(
    kind: UpdatePackageKind,
    path: &Path,
) -> Result<(String, Vec<String>, String), String> {
    let path = path
        .to_str()
        .ok_or_else(|| "update package path is not valid UTF-8".to_string())?
        .to_string();
    let (program, args) = match kind {
        UpdatePackageKind::Deb => {
            if command_exists("apt") {
                ("apt".to_string(), vec!["install".to_string(), path])
            } else if command_exists("dpkg") {
                ("dpkg".to_string(), vec!["-i".to_string(), path])
            } else {
                return Err("apt/dpkg bulunamadı. .deb paketini elle kurun.".to_string());
            }
        }
        UpdatePackageKind::Rpm => {
            if command_exists("dnf") {
                ("dnf".to_string(), vec!["install".to_string(), path])
            } else if command_exists("yum") {
                ("yum".to_string(), vec!["localinstall".to_string(), path])
            } else if command_exists("zypper") {
                ("zypper".to_string(), vec!["install".to_string(), path])
            } else if command_exists("rpm") {
                ("rpm".to_string(), vec!["-Uvh".to_string(), path])
            } else {
                return Err("dnf/yum/zypper/rpm bulunamadı. .rpm paketini elle kurun.".to_string());
            }
        }
        UpdatePackageKind::Pacman => {
            if command_exists("pacman") {
                ("pacman".to_string(), vec!["-U".to_string(), path])
            } else {
                return Err("pacman bulunamadı. .pkg.tar.zst paketini elle kurun.".to_string());
            }
        }
        UpdatePackageKind::Tarball => {
            return Err("Tarball otomatik kurulum için desteklenmiyor.".to_string());
        }
        UpdatePackageKind::AppImage | UpdatePackageKind::WindowsMsi => {
            return Err("Bu paket türü Linux paket yöneticisiyle kurulamaz.".to_string());
        }
    };

    let (launcher, launcher_args) = elevate_command(program, args)?;
    let preview = std::iter::once(launcher.clone())
        .chain(launcher_args.iter().cloned())
        .map(|part| shell_quote(&part))
        .collect::<Vec<String>>()
        .join(" ");
    Ok((launcher, launcher_args, preview))
}

#[cfg(unix)]
fn elevate_command(program: String, args: Vec<String>) -> Result<(String, Vec<String>), String> {
    if process_is_root() {
        return Ok((program, args));
    }

    if command_exists("sudo") {
        let mut elevated_args = Vec::with_capacity(args.len() + 1);
        elevated_args.push(program);
        elevated_args.extend(args);
        return Ok(("sudo".to_string(), elevated_args));
    }

    if command_exists("pkexec") {
        let mut elevated_args = Vec::with_capacity(args.len() + 1);
        elevated_args.push(program);
        elevated_args.extend(args);
        return Ok(("pkexec".to_string(), elevated_args));
    }

    Err("Yetki yükseltme aracı bulunamadı. pkexec veya sudo kurulu olmalı.".to_string())
}

fn command_exists(program: &str) -> bool {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(program).is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '+'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn launch_update_installer(path: &Path) -> Result<String, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let package_kind = UpdatePackageKind::from_asset_name(file_name)
        .unwrap_or_else(|| current_update_target().kind);

    #[cfg(windows)]
    {
        let mut command = if package_kind == UpdatePackageKind::WindowsMsi {
            let mut command = Command::new("msiexec");
            command.arg("/i").arg(path).arg("/passive");
            command
        } else {
            Command::new(path)
        };
        command
            .spawn()
            .map_err(|err| format!("installer could not be started: {err}"))?;
        return Ok("installer started".to_string());
    }

    #[cfg(unix)]
    {
        if package_kind == UpdatePackageKind::AppImage {
            make_executable(path)?;
            Command::new(path)
                .spawn()
                .map_err(|err| format!("installer could not be started: {err}"))?;
            return Ok("AppImage başlatıldı.".to_string());
        }

        let (program, args, preview) = linux_package_install_command(package_kind, path)?;
        let _ = (program, args);
        let log_path = update_install_log_path(path);
        let script = linux_update_console_script(&preview, &log_path);
        let (terminal, terminal_args) = terminal_command_for_script(&script).ok_or_else(|| {
            format!("Kurulum terminali bulunamadı. Şu komutu terminalde elle çalıştırın: {preview}")
        })?;
        Command::new(&terminal)
            .args(&terminal_args)
            .spawn()
            .map_err(|err| format!("installer console could not be started: {err}"))?;
        Ok(format!(
            "Kurulum konsolu açıldı: {preview}\nLog: {}",
            log_path.display()
        ))
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = package_kind;
        Err("automatic update install is not supported on this platform".to_string())
    }
}

#[cfg(unix)]
fn update_install_log_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("amele-update");
    path.with_file_name(format!("{file_name}.install.log"))
}

#[cfg(unix)]
fn linux_update_console_script(command_line: &str, log_path: &Path) -> String {
    let log = shell_quote(&log_path.to_string_lossy());
    format!(
        "set -o pipefail\n\
         clear\n\
         echo 'Amele güncelleme kurulumu'\n\
         echo 'Komut: {command_line}'\n\
         echo 'Log: {log}'\n\
         echo\n\
         ({command_line}) 2>&1 | tee -a {log}\n\
         status=${{PIPESTATUS[0]}}\n\
         echo\n\
         echo \"Çıkış kodu: $status\" | tee -a {log}\n\
         if [ \"$status\" -eq 0 ]; then echo 'Kurulum tamamlandı.' | tee -a {log}; else echo 'Kurulum başarısız oldu.' | tee -a {log}; fi\n\
         echo\n\
         read -r -p 'Kapatmak için Enter...' _\n\
         exit \"$status\""
    )
}

#[cfg(unix)]
fn terminal_command_for_script(script: &str) -> Option<(String, Vec<String>)> {
    let candidates = [
        "x-terminal-emulator",
        "kgx",
        "gnome-terminal",
        "konsole",
        "alacritty",
        "kitty",
        "xterm",
    ];
    candidates.iter().find_map(|terminal| {
        if !command_exists(terminal) {
            return None;
        }
        let args = match *terminal {
            "kgx" | "gnome-terminal" => vec!["--", "bash", "-lc", script],
            _ => vec!["-e", "bash", "-lc", script],
        };
        Some((
            (*terminal).to_string(),
            args.into_iter().map(str::to_string).collect(),
        ))
    })
}

fn sanitize_download_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn default_download_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
}

#[cfg(test)]
mod tests {
    use super::{
        UpdatePackageKind, UpdateTarget, preferred_update_asset, shell_quote,
        update_install_preview,
    };
    use serde_json::json;

    #[test]
    fn update_asset_selection_respects_linux_package_kind() {
        let assets = vec![
            json!({"name": "amele-linux-x64.AppImage", "download_url": "appimage"}),
            json!({"name": "amele-linux-x64.deb", "download_url": "deb"}),
            json!({"name": "amele-linux-x64.rpm", "download_url": "rpm"}),
            json!({"name": "amele-linux-x64.pkg.tar.zst", "download_url": "arch"}),
        ];

        let arch = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::Pacman,
                detected_by: "test",
            },
        );
        assert_eq!(
            arch.get("name").and_then(|value| value.as_str()),
            Some("amele-linux-x64.pkg.tar.zst")
        );
        assert_eq!(
            arch.get("package_kind").and_then(|value| value.as_str()),
            Some("pacman")
        );

        let deb = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::Deb,
                detected_by: "test",
            },
        );
        assert_eq!(
            deb.get("name").and_then(|value| value.as_str()),
            Some("amele-linux-x64.deb")
        );

        let rpm = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::Rpm,
                detected_by: "test",
            },
        );
        assert_eq!(
            rpm.get("name").and_then(|value| value.as_str()),
            Some("amele-linux-x64.rpm")
        );
    }

    #[test]
    fn update_asset_selection_accepts_legacy_arch_tar_zst_name() {
        let assets = vec![
            json!({"name": "amele-linux-x64.AppImage", "download_url": "appimage"}),
            json!({"name": "amele-linux-x64.tar.zst", "download_url": "arch"}),
        ];

        let arch = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::Pacman,
                detected_by: "pacman-owner",
            },
        );
        assert_eq!(
            arch.get("name").and_then(|value| value.as_str()),
            Some("amele-linux-x64.tar.zst")
        );
        assert_eq!(
            arch.get("package_kind").and_then(|value| value.as_str()),
            Some("pacman")
        );
        assert_eq!(
            UpdatePackageKind::from_asset_name("generic-linux-x64.tar.zst"),
            None
        );
    }

    #[test]
    fn update_asset_selection_does_not_fallback_for_detected_linux_package() {
        let assets = vec![
            json!({"name": "amele-linux-x64.AppImage", "download_url": "appimage"}),
            json!({"name": "amele-linux-x64.deb", "download_url": "deb"}),
        ];

        let arch = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::Pacman,
                detected_by: "pacman-owner",
            },
        );
        assert!(arch.is_null());

        let fallback = preferred_update_asset(
            &assets,
            &UpdateTarget {
                kind: UpdatePackageKind::AppImage,
                detected_by: "fallback",
            },
        );
        assert_eq!(
            fallback.get("name").and_then(|value| value.as_str()),
            Some("amele-linux-x64.AppImage")
        );
    }

    #[test]
    fn update_install_preview_uses_package_manager_commands() {
        assert_eq!(
            update_install_preview(
                UpdatePackageKind::Pacman,
                Some("amele-linux-x64.pkg.tar.zst")
            ),
            "sudo pacman -U amele-linux-x64.pkg.tar.zst"
        );
        assert_eq!(
            update_install_preview(UpdatePackageKind::Deb, Some("amele-linux-x64.deb")),
            "sudo apt install amele-linux-x64.deb"
        );
        assert_eq!(
            update_install_preview(UpdatePackageKind::Rpm, Some("amele-linux-x64.rpm")),
            "sudo dnf install amele-linux-x64.rpm"
        );
    }

    #[test]
    fn shell_quote_handles_spaces() {
        assert_eq!(
            shell_quote("/tmp/amele package.deb"),
            "'/tmp/amele package.deb'"
        );
    }
}
