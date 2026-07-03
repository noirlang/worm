//! Android cihazlardan mantıksal veri, medya, log ve manifest çıktıları toplar.
use super::adb::{
    first_non_empty, run_adb_command, run_adb_command_timeout, run_adb_file_command,
    run_adb_file_command_timeout,
};
use super::app_catalog::{ANDROID_APP_TARGETS, AndroidAppTarget};
use super::extractors::{AndroidAcquisitionProfile, logical_steps_for_profile};
use serde::Serialize;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Logical acquisition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
/// Tek bir Android toplama adımının durumunu ve çıktı dosyasını taşır.
pub struct AcquisitionItem {
    pub category: String,
    pub file_name: String,
    pub size: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
/// Mantıksal Android ediniminin klasör, hash ve adım listesini döndürür.
pub struct LogicalAcquisitionResult {
    pub output_dir: std::path::PathBuf,
    pub items: Vec<AcquisitionItem>,
    pub total_bytes: u64,
    pub sha256: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
/// pm list packages çıktısından normalize edilen paket kaydıdır.
struct ParsedPackage {
    package: String,
    apk_path: Option<String>,
    uid: Option<String>,
    version_code: Option<String>,
    raw: String,
}

#[derive(Debug, Clone, Serialize)]
/// AccountManager çıktısından çıkarılan hesap özetidir.
struct SocialAccountRecord {
    provider: String,
    account_type: String,
    account_name: String,
    confidence: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
/// Kurulu sosyal/iletişim uygulamasını paket bilgisiyle taşır.
struct SocialAppRecord {
    platform: String,
    package: String,
    category: String,
    priority: String,
    storage_hint: String,
    apk_path: Option<String>,
    uid: Option<String>,
    version_code: Option<String>,
    installed: bool,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
/// Çalışan sosyal/iletişim uygulaması sürecini temsil eder.
struct SocialProcessRecord {
    platform: String,
    package: String,
    category: String,
    pid: u32,
    process_name: String,
    user: Option<String>,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
/// Non-root sosyal medya edinimi için kaynak ve sınırları birlikte raporlar.
struct SocialSummary<T> {
    source: String,
    limitation: String,
    records: Vec<T>,
}

#[derive(Debug, Clone, Serialize)]
/// Türkiye odaklı uygulama depolama taramasındaki tek yol sonucudur.
struct AppStoragePathProbe {
    path: String,
    exists: bool,
    size_kb: Option<u64>,
    sample_files: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
/// Kurulu hedef uygulamanın erişilebilir ve root gerektiren depolama haritasıdır.
struct TurkeyAppStorageRecord {
    platform: String,
    package: String,
    category: String,
    priority: String,
    installed: bool,
    apk_path: Option<String>,
    uid: Option<String>,
    version_code: Option<String>,
    public_paths: Vec<AppStoragePathProbe>,
    root_required_paths: Vec<String>,
    storage_hint: String,
    acquisition_note: String,
}

#[derive(Debug, Clone, Serialize)]
/// Türkiye uygulama hedefleri için genel depolama raporu.
struct TurkeyAppStorageSummary {
    source: String,
    limitation: String,
    root_available: bool,
    records: Vec<TurkeyAppStorageRecord>,
    not_installed_high_priority_targets: Vec<String>,
}

/// ADB shell komutunun çıktısını dosyaya yazar ve sonucu standart edinim kaydına çevirir.
fn collect_shell_output(
    serial: &str,
    category: &str,
    file_name: &str,
    shell_args: &[&str],
    dir: &std::path::Path,
) -> AcquisitionItem {
    match run_adb_command(serial, shell_args) {
        Ok(content) => {
            let path = dir.join(file_name);
            match std::fs::write(&path, &content) {
                Ok(()) => AcquisitionItem {
                    category: category.to_string(),
                    file_name: file_name.to_string(),
                    size: content.len() as u64,
                    success: true,
                    error: None,
                },
                Err(err) => AcquisitionItem {
                    category: category.to_string(),
                    file_name: file_name.to_string(),
                    size: 0,
                    success: false,
                    error: Some(format!("Dosya yazilamadi: {err}")),
                },
            }
        }
        Err(err) => AcquisitionItem {
            category: category.to_string(),
            file_name: file_name.to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Cihazın getprop çıktısını toplayarak temel cihaz bilgisini kaydeder.
fn collect_device_info(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "device_info",
        "device_info.txt",
        &["shell", "getprop"],
        dir,
    )
}

/// Kurulu paket listesini dosya yollarıyla birlikte toplar.
fn collect_packages(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "packages",
        "packages.txt",
        &["shell", "pm", "list", "packages", "-f"],
        dir,
    )
}

/// Paket listesini JSON formatında normalize ederek analize hazır hale getirir.
fn collect_packages_json(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    match run_adb_command(
        serial,
        &[
            "shell",
            "pm list packages -f -U --show-versioncode 2>/dev/null || pm list packages -f",
        ],
    ) {
        Ok(output) => {
            let packages = parse_package_rows(&output);
            let path = dir.join("packages.json");
            match serde_json::to_vec_pretty(&packages)
                .map_err(|err| err.to_string())
                .and_then(|content| {
                    std::fs::write(&path, &content)
                        .map(|_| content.len() as u64)
                        .map_err(|err| err.to_string())
                }) {
                Ok(size) => AcquisitionItem {
                    category: "packages_json".to_string(),
                    file_name: "packages.json".to_string(),
                    size,
                    success: true,
                    error: None,
                },
                Err(err) => AcquisitionItem {
                    category: "packages_json".to_string(),
                    file_name: "packages.json".to_string(),
                    size: 0,
                    success: false,
                    error: Some(format!("Paket JSON yazilamadi: {err}")),
                },
            }
        }
        Err(err) => AcquisitionItem {
            category: "packages_json".to_string(),
            file_name: "packages.json".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Android paket satırlarını JSON modeline ayrıştırır.
fn parse_package_rows(output: &str) -> Vec<ParsedPackage> {
    output
        .lines()
        .filter_map(|line| {
            let raw = line.trim();
            let rest = raw.strip_prefix("package:")?;
            let mut parts = rest.split_whitespace();
            let first = parts.next().unwrap_or_default();
            let (apk_path, package) = first
                .rsplit_once('=')
                .map(|(path, package)| (Some(path.to_string()), package.to_string()))
                .unwrap_or_else(|| (None, first.to_string()));
            if package.is_empty() {
                return None;
            }

            let mut uid = None;
            let mut version_code = None;
            for part in parts {
                if let Some(value) = part.strip_prefix("uid:") {
                    uid = Some(value.to_string());
                } else if let Some(value) = part.strip_prefix("versionCode:") {
                    version_code = Some(value.to_string());
                }
            }

            Some(ParsedPackage {
                package,
                apk_path,
                uid,
                version_code,
                raw: raw.to_string(),
            })
        })
        .collect()
}

/// Paket envanterini mümkünse ayrıntılı komuttan, olmazsa mevcut dosyadan okur.
fn load_package_inventory(
    serial: &str,
    dir: &std::path::Path,
) -> Result<Vec<ParsedPackage>, String> {
    let output = run_adb_command(
        serial,
        &[
            "shell",
            "pm list packages -f -U --show-versioncode 2>/dev/null || pm list packages -f",
        ],
    )
    .or_else(|_| {
        std::fs::read_to_string(dir.join("packages.txt")).map_err(|err| err.to_string())
    })?;

    Ok(parse_package_rows(&output))
}

/// Katalogdaki sosyal/iletişim uygulamalarını döndürür.
fn social_targets() -> impl Iterator<Item = &'static AndroidAppTarget> {
    ANDROID_APP_TARGETS.iter().filter(|target| {
        matches!(
            target.category,
            "account_mail"
                | "account_identity"
                | "social"
                | "social_video"
                | "messaging"
                | "messaging_tr"
                | "social_business"
        )
    })
}

/// Kurulu sosyal/iletişim uygulamalarını paket listesinden özetler.
fn collect_social_apps(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let packages = load_package_inventory(serial, dir);

    match packages {
        Ok(packages) => {
            let mut records = Vec::new();
            for target in social_targets() {
                if let Some(package) = packages.iter().find(|pkg| pkg.package == target.package) {
                    records.push(SocialAppRecord {
                        platform: target.platform.to_string(),
                        package: package.package.clone(),
                        category: target.category.to_string(),
                        priority: target.priority.to_string(),
                        storage_hint: target.storage_hint.to_string(),
                        apk_path: package.apk_path.clone(),
                        uid: package.uid.clone(),
                        version_code: package.version_code.clone(),
                        installed: true,
                        source: "pm list packages".to_string(),
                    });
                }
            }

            let summary = SocialSummary {
                source: "pm list packages -f -U --show-versioncode".to_string(),
                limitation: "Bu çıktı kurulu sosyal/iletişim uygulamalarını gösterir; giriş yapılmış kullanıcı adını garanti etmez.".to_string(),
                records,
            };
            write_json_acquisition_item(dir, "social_apps", "social_apps.json", &summary)
        }
        Err(err) => AcquisitionItem {
            category: "social_apps".to_string(),
            file_name: "social_apps.json".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Türkiye'de sık karşılaşılan uygulamalar için erişilebilir depolama izlerini raporlar.
fn collect_turkey_app_storage(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let packages = match load_package_inventory(serial, dir) {
        Ok(packages) => packages,
        Err(err) => {
            return AcquisitionItem {
                category: "turkey_app_storage".to_string(),
                file_name: "turkey_app_storage.json".to_string(),
                size: 0,
                success: false,
                error: Some(format!("Paket envanteri okunamadi: {err}")),
            };
        }
    };

    let root_available = android_root_available(serial);
    let mut records = Vec::new();
    let mut not_installed_high_priority_targets = Vec::new();

    for target in ANDROID_APP_TARGETS {
        let installed_package = packages.iter().find(|pkg| pkg.package == target.package);
        if installed_package.is_none() {
            if target.priority == "turkey_high" {
                not_installed_high_priority_targets
                    .push(format!("{} ({})", target.platform, target.package));
            }
            continue;
        }

        let package = installed_package.expect("checked above");
        let public_paths = target_public_storage_paths(target)
            .into_iter()
            .map(|path| probe_public_storage_path(serial, &path))
            .collect();
        let root_required_paths = target_root_storage_paths(target);

        records.push(TurkeyAppStorageRecord {
            platform: target.platform.to_string(),
            package: package.package.clone(),
            category: target.category.to_string(),
            priority: target.priority.to_string(),
            installed: true,
            apk_path: package.apk_path.clone(),
            uid: package.uid.clone(),
            version_code: package.version_code.clone(),
            public_paths,
            root_required_paths,
            storage_hint: target.storage_hint.to_string(),
            acquisition_note: if root_available {
                "Root mevcut görünüyor; private app dizinleri ayrı root/file-system edinim adımında hedeflenebilir.".to_string()
            } else {
                "Root yok veya onaylanmadı; Android 10+ cihazlarda /data/user/0 ve diğer uygulamaların app-specific dizinleri ADB non-root ile okunamaz.".to_string()
            },
        });
    }

    let summary = TurkeyAppStorageSummary {
        source: "pm list packages + /sdcard app-specific path probes".to_string(),
        limitation: "Bu rapor Türkiye'de sık kullanılan hedef uygulamalar için kurulum ve erişilebilir dış depolama izlerini gösterir. Private uygulama verileri root, üretici yedeği veya yasal/cihaz sahibi onayı gerektirebilir.".to_string(),
        root_available,
        records,
        not_installed_high_priority_targets,
    };

    write_json_acquisition_item(
        dir,
        "turkey_app_storage",
        "turkey_app_storage.json",
        &summary,
    )
}

/// Android root erişimini kısa zaman aşımıyla yoklar.
fn android_root_available(serial: &str) -> bool {
    run_adb_command_timeout(serial, &["shell", "su -c id"], Duration::from_secs(3))
        .map(|output| output.contains("uid=0") || output.contains("root"))
        .unwrap_or(false)
}

/// Hedef uygulama için non-root denenebilecek dış depolama yollarını üretir.
fn target_public_storage_paths(target: &AndroidAppTarget) -> Vec<String> {
    let mut paths = vec![
        format!("/sdcard/Android/media/{}", target.package),
        format!("/sdcard/Android/data/{}", target.package),
        format!("/sdcard/Android/obb/{}", target.package),
    ];

    match target.package {
        "com.whatsapp" => {
            paths.push("/sdcard/WhatsApp/Media".to_string());
            paths.push("/sdcard/Pictures/WhatsApp".to_string());
        }
        "com.whatsapp.w4b" => {
            paths.push("/sdcard/WhatsApp Business/Media".to_string());
        }
        "org.telegram.messenger" => {
            paths.push("/sdcard/Telegram".to_string());
        }
        "com.instagram.android" => {
            paths.push("/sdcard/Pictures/Instagram".to_string());
            paths.push("/sdcard/Movies/Instagram".to_string());
        }
        "com.twitter.android" => {
            paths.push("/sdcard/Pictures/Twitter".to_string());
            paths.push("/sdcard/Download/Twitter".to_string());
        }
        "com.zhiliaoapp.musically" => {
            paths.push("/sdcard/Movies/TikTok".to_string());
            paths.push("/sdcard/Download/TikTok".to_string());
        }
        "com.turkcell.bip" => {
            paths.push("/sdcard/BiP".to_string());
        }
        _ => {}
    }

    paths
}

/// Rootlu dosya sistemi ediniminde hedeflenmesi gereken private dizinleri üretir.
fn target_root_storage_paths(target: &AndroidAppTarget) -> Vec<String> {
    vec![
        format!("/data/user/0/{}", target.package),
        format!("/data/data/{}", target.package),
        format!("/data_mirror/data_ce/null/0/{}", target.package),
    ]
}

/// Tek dış depolama yolunu varlık, boyut ve örnek dosya listesiyle yoklar.
fn probe_public_storage_path(serial: &str, path: &str) -> AppStoragePathProbe {
    let quoted = adb_shell_quote(path);
    let command = format!(
        "if [ -e {quoted} ]; then echo __EXISTS__; du -sk {quoted} 2>/dev/null | tail -n 1; find {quoted} -maxdepth 2 -type f 2>/dev/null | head -n 12; else echo __MISSING__; fi"
    );

    match run_adb_command_timeout(serial, &["shell", &command], Duration::from_secs(8)) {
        Ok(output) => parse_storage_probe_output(path, &output),
        Err(err) => AppStoragePathProbe {
            path: path.to_string(),
            exists: false,
            size_kb: None,
            sample_files: Vec::new(),
            error: Some(err),
        },
    }
}

/// ADB shell için tek tırnaklı güvenli argüman üretir.
fn adb_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Depolama yoklama çıktısını yapılandırılmış modele çevirir.
fn parse_storage_probe_output(path: &str, output: &str) -> AppStoragePathProbe {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return AppStoragePathProbe {
            path: path.to_string(),
            exists: false,
            size_kb: None,
            sample_files: Vec::new(),
            error: Some("Boş ADB yanıtı".to_string()),
        };
    };

    if first == "__MISSING__" {
        return AppStoragePathProbe {
            path: path.to_string(),
            exists: false,
            size_kb: None,
            sample_files: Vec::new(),
            error: None,
        };
    }

    let mut size_kb = None;
    let mut sample_files = Vec::new();
    for line in lines {
        if size_kb.is_none() {
            size_kb = line.split_whitespace().next().and_then(|v| v.parse().ok());
            if size_kb.is_some() {
                continue;
            }
        }
        sample_files.push(line.to_string());
    }

    AppStoragePathProbe {
        path: path.to_string(),
        exists: first == "__EXISTS__",
        size_kb,
        sample_files,
        error: None,
    }
}

/// AccountManager üzerinden görülebilen Google ve sosyal hesapları yapılandırılmış JSON'a çevirir.
fn collect_social_accounts(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let output = std::fs::read_to_string(dir.join("dumpsys_account.txt"))
        .or_else(|_| run_adb_command(serial, &["shell", "dumpsys", "account"]));

    match output {
        Ok(output) => {
            let mut records = Vec::new();
            for line in output.lines() {
                let trimmed = line.trim();
                let Some(account) = parse_account_line(trimmed) else {
                    continue;
                };
                let provider = classify_account_provider(&account.1);
                if provider == "other" && !looks_like_email_or_social_account(&account.0) {
                    continue;
                }
                records.push(SocialAccountRecord {
                    provider,
                    account_name: account.0,
                    account_type: account.1,
                    confidence: "account_manager".to_string(),
                    source: "dumpsys account".to_string(),
                });
            }

            let summary = SocialSummary {
                source: "dumpsys account / Android AccountManager".to_string(),
                limitation: "Google e-posta ve AccountManager'a kayıtlı uygulama hesapları görülebilir; Instagram/X gibi private app oturumları root olmadan genelde görünmez.".to_string(),
                records,
            };
            write_json_acquisition_item(dir, "social_accounts", "social_accounts.json", &summary)
        }
        Err(err) => AcquisitionItem {
            category: "social_accounts".to_string(),
            file_name: "social_accounts.json".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Çalışan süreçlerden sosyal/iletişim uygulamalarını ayıklar.
fn collect_social_processes(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let output = std::fs::read_to_string(dir.join("processes.txt"))
        .or_else(|_| run_adb_command(serial, &["shell", "ps", "-A"]));

    match output {
        Ok(output) => {
            let mut records = Vec::new();
            for line in output.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let Some(name) = parts.last().copied() else {
                    continue;
                };
                let Some(target) = social_package_for_process(name) else {
                    continue;
                };
                let pid = parts
                    .iter()
                    .find_map(|part| part.parse::<u32>().ok())
                    .unwrap_or_default();
                records.push(SocialProcessRecord {
                    platform: target.platform.to_string(),
                    package: target.package.to_string(),
                    category: target.category.to_string(),
                    pid,
                    process_name: name.to_string(),
                    user: parts.first().map(|value| (*value).to_string()),
                    source: "ps -A".to_string(),
                });
            }

            let summary = SocialSummary {
                source: "ps -A".to_string(),
                limitation: "Bu çıktı uygulamanın çalışan sürecini gösterir; tek başına hesap kullanıcı adını kanıtlamaz.".to_string(),
                records,
            };
            write_json_acquisition_item(dir, "social_processes", "social_processes.json", &summary)
        }
        Err(err) => AcquisitionItem {
            category: "social_processes".to_string(),
            file_name: "social_processes.json".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

fn parse_account_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("Account {")?.strip_suffix('}')?;
    let mut name = None;
    let mut account_type = None;
    for part in rest.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("name=") {
            name = Some(value.trim().to_string());
        } else if let Some(value) = part.strip_prefix("type=") {
            account_type = Some(value.trim().to_string());
        }
    }
    Some((name?, account_type?))
}

fn classify_account_provider(account_type: &str) -> String {
    let lower = account_type.to_ascii_lowercase();
    if lower.contains("google") {
        "google".to_string()
    } else if lower.contains("samsung") {
        "samsung".to_string()
    } else if lower.contains("whatsapp") {
        "whatsapp".to_string()
    } else if lower.contains("telegram") {
        "telegram".to_string()
    } else if lower.contains("facebook") {
        "facebook".to_string()
    } else if lower.contains("twitter") || lower.contains(".x") {
        "x_twitter".to_string()
    } else if lower.contains("instagram") {
        "instagram".to_string()
    } else if lower.contains("microsoft") {
        "microsoft".to_string()
    } else {
        "other".to_string()
    }
}

fn looks_like_email_or_social_account(value: &str) -> bool {
    value.contains('@') || value.starts_with('+') || value.chars().any(|ch| ch.is_ascii_digit())
}

fn social_package_for_process(process_name: &str) -> Option<&'static AndroidAppTarget> {
    social_targets().find(|target| {
        process_name == target.package || process_name.starts_with(&format!("{}:", target.package))
    })
}

/// Cihazdaki mevcut logcat tamponunu metin çıktısı olarak alır.
fn collect_logcat(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(serial, "logcat", "logcat.txt", &["logcat", "-d"], dir)
}

/// Kernel ve Android log tamponlarını tek dosyada toplar.
fn collect_system_logs(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "system_logs",
        "system_logs.txt",
        &[
            "shell",
            "echo '=== logcat -b all ==='; logcat -d -b all 2>/dev/null; echo '=== dmesg ==='; dmesg 2>/dev/null || true",
        ],
        dir,
    )
}

/// Belirli dumpsys servisini çalıştırıp çıktısını ilgili dosyaya yazar.
fn collect_dumpsys(
    serial: &str,
    service: &str,
    file_name: &str,
    dir: &std::path::Path,
) -> AcquisitionItem {
    let category = format!("dumpsys_{service}");
    collect_shell_output(
        serial,
        &category,
        file_name,
        &["shell", "dumpsys", service],
        dir,
    )
}

/// Android bugreport çıktısını vaka klasöründe zip olarak üretir.
fn collect_bugreport(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let target_path = dir.join("bugreport.zip");
    let target = target_path.to_string_lossy().into_owned();
    match run_adb_file_command(serial, &["bugreport", &target]) {
        Ok(()) => {
            // Üretilen bugreport dosyasını bulup gerçek boyutunu raporlarız.
            let mut found: Option<std::path::PathBuf> = None;
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with("bugreport") && name.ends_with(".zip") {
                        found = Some(entry.path());
                        break;
                    }
                }
            }
            let (file_name, size) = match found {
                Some(path) => {
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "bugreport.zip".to_string());
                    (name, size)
                }
                None => ("bugreport.zip".to_string(), 0),
            };
            AcquisitionItem {
                category: "bugreport".to_string(),
                file_name,
                size,
                success: true,
                error: None,
            }
        }
        Err(err) => AcquisitionItem {
            category: "bugreport".to_string(),
            file_name: "bugreport.zip".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Kullanıcıya açık /sdcard depolama alanını adb pull ile toplar.
fn collect_shared_storage(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let target = dir.join("shared_storage");
    let _ = std::fs::create_dir_all(&target);
    let target_str = target.to_string_lossy().into_owned();
    match run_adb_file_command(serial, &["pull", "/sdcard/", &target_str]) {
        Ok(()) => {
            let size = dir_size(&target);
            AcquisitionItem {
                category: "shared_storage".to_string(),
                file_name: "shared_storage".to_string(),
                size,
                success: true,
                error: None,
            }
        }
        Err(err) => {
            // Kısmi pull çıktısı da delil değeri taşıyebileceği için boyutuyla raporlanır.
            let size = dir_size(&target);
            AcquisitionItem {
                category: "shared_storage".to_string(),
                file_name: "shared_storage".to_string(),
                size,
                success: size > 0,
                error: if size > 0 {
                    Some(format!("Kismi basarili: {err}"))
                } else {
                    Some(err)
                },
            }
        }
    }
}

/// Bildirim geçmişi ve aktif bildirim dökümünü olabildiğince ayrıntılı alır.
fn collect_notification_history(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let mut content = String::new();

    // Mesaj metinleri maskelenmesin diye noredact seçeneği denenir.
    content.push_str("=== dumpsys notification --noredact ===\n");
    match run_adb_command(serial, &["shell", "dumpsys", "notification", "--noredact"]) {
        Ok(output) => content.push_str(&output),
        Err(err) => content.push_str(&format!("Hata: {err}")),
    }
    content.push_str("\n\n");

    // Bildirim geçmişi açıksa ayrı geçmiş dökümü de alınır.
    let history_enabled = run_adb_command(
        serial,
        &[
            "shell",
            "settings",
            "get",
            "secure",
            "notification_history_enabled",
        ],
    )
    .map(|v| v.trim() == "1")
    .unwrap_or(false);

    if history_enabled {
        content.push_str("=== Bildirim gecmisi etkin — notification history ===\n");
        match run_adb_command(serial, &["shell", "cmd", "notification", "dump_history"]) {
            Ok(output) => content.push_str(&output),
            Err(err) => content.push_str(&format!("Hata: {err}")),
        }
    } else {
        content.push_str(
            "=== Bildirim gecmisi etkin degil (settings > notification_history_enabled != 1) ===\n",
        );
    }

    let path = dir.join("dumpsys_notification.txt");
    match std::fs::write(&path, &content) {
        Ok(()) => AcquisitionItem {
            category: "dumpsys_notification".to_string(),
            file_name: "dumpsys_notification.txt".to_string(),
            size: content.len() as u64,
            success: true,
            error: None,
        },
        Err(err) => AcquisitionItem {
            category: "dumpsys_notification".to_string(),
            file_name: "dumpsys_notification.txt".to_string(),
            size: 0,
            success: false,
            error: Some(format!("Dosya yazilamadi: {err}")),
        },
    }
}

/// IP, rota, soket ve komşu ağ kayıtlarını tek ağ özeti dosyasında toplar.
fn collect_network_info(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let commands = [
        ("=== ip addr ===", vec!["shell", "ip", "addr"]),
        ("=== ip route ===", vec!["shell", "ip", "route"]),
        ("=== netstat ===", vec!["shell", "netstat", "-tlnp"]),
        ("=== ip neigh ===", vec!["shell", "ip", "neigh"]),
    ];
    let mut content = String::new();
    for (header, args) in &commands {
        content.push_str(header);
        content.push('\n');
        match run_adb_command(serial, args.as_slice()) {
            Ok(output) => content.push_str(&output),
            Err(err) => content.push_str(&format!("Hata: {err}")),
        }
        content.push_str("\n\n");
    }
    let path = dir.join("network_info.txt");
    match std::fs::write(&path, &content) {
        Ok(()) => AcquisitionItem {
            category: "network_info".to_string(),
            file_name: "network_info.txt".to_string(),
            size: content.len() as u64,
            success: true,
            error: None,
        },
        Err(err) => AcquisitionItem {
            category: "network_info".to_string(),
            file_name: "network_info.txt".to_string(),
            size: 0,
            success: false,
            error: Some(format!("Dosya yazilamadi: {err}")),
        },
    }
}

/// Cihazdaki çalışan proses listesini ps çıktısı olarak kaydeder.
fn collect_processes(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "processes",
        "processes.txt",
        &["shell", "ps", "-A"],
        dir,
    )
}

/// service list çıktısıyla sistem servislerini kaydeder.
fn collect_services(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "services",
        "services.txt",
        &["shell", "service list"],
        dir,
    )
}

/// Mount tablosunu ve depolama bağlama noktalarını kaydeder.
fn collect_mounts(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "mounts",
        "mounts.txt",
        &["shell", "cat /proc/mounts; echo '=== mount ==='; mount"],
        dir,
    )
}

/// SELinux durumunu ve enforce değerini kaydeder.
fn collect_selinux_status(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "selinux_status",
        "selinux_status.txt",
        &[
            "shell",
            "getenforce 2>/dev/null; cat /sys/fs/selinux/enforce 2>/dev/null || true",
        ],
        dir,
    )
}

/// Shell ortamı, kullanıcı kimliği ve kernel özetini kaydeder.
fn collect_environment(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "environment",
        "environment.txt",
        &[
            "shell",
            "id; uname -a; printenv 2>/dev/null; getprop ro.build.fingerprint",
        ],
        dir,
    )
}

/// Bilinen root araçlarının izlerini arar.
fn collect_root_binaries(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "root_binaries",
        "root_binaries.txt",
        &[
            "shell",
            "for p in /system/bin/su /system/xbin/su /sbin/su /vendor/bin/su /su/bin/su /data/adb/magisk /data/adb/ksu/bin/su /system/bin/magisk /system/xbin/busybox; do [ -e \"$p\" ] && ls -la \"$p\"; done; command -v su 2>/dev/null || true; command -v magisk 2>/dev/null || true",
        ],
        dir,
    )
}

/// Geçici dizinlerdeki son kullanıcı ve araç izlerini listeler.
fn collect_temp_files(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "temp_files",
        "temp_files.txt",
        &[
            "shell",
            "echo '=== /data/local/tmp ==='; ls -laR /data/local/tmp 2>/dev/null; echo '=== /sdcard/Download ==='; ls -laR /sdcard/Download 2>/dev/null; echo '=== /sdcard/Documents ==='; ls -laR /sdcard/Documents 2>/dev/null",
        ],
        dir,
    )
}

/// Root, hook, debug ve ağ izlerini hızlı IOC taraması olarak toplar.
fn collect_intrusion_indicators(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "intrusion_indicators",
        "intrusion_indicators.txt",
        &[
            "shell",
            "echo '=== processes ==='; ps -A | grep -Ei 'frida|magisk|zygisk|xposed|substrate|su|busybox|tcpdump|gadget|objection' || true; echo '=== packages ==='; pm list packages | grep -Ei 'frida|magisk|xposed|substrate|superuser|kingroot|kernelsu|lsposed|zygisk' || true; echo '=== logcat indicators ==='; logcat -d -b all 2>/dev/null | grep -Ei 'frida|magisk|zygisk|xposed|substrate|root|su|selinux|denied' | tail -n 1000 || true",
        ],
        dir,
    )
}

/// Paylaşımlı depolama üzerinde sınırlı derinlikte dosya indeksi üretir.
fn collect_file_index(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "file_index",
        "file_index.txt",
        &[
            "shell",
            "find /sdcard -maxdepth 4 -type f 2>/dev/null | head -n 10000",
        ],
        dir,
    )
}

/// Dosya sistemi doluluk bilgisini df çıktısı olarak toplar.
fn collect_disk_usage(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_shell_output(
        serial,
        "disk_usage",
        "disk_usage.txt",
        &["shell", "df", "-h"],
        dir,
    )
}

/// Ekran görüntüsünü cihazda üretip vaka klasörüne çeker.
fn collect_screenshot(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let remote_path = "/sdcard/amele_screenshot.png";
    // Önce cihaz tarafında geçici ekran görüntüsü oluşturulur.
    if let Err(err) = run_adb_command(serial, &["shell", "screencap", "-p", remote_path]) {
        return AcquisitionItem {
            category: "screenshot".to_string(),
            file_name: "screenshot.png".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        };
    }
    let local_path = dir.join("screenshot.png");
    let local_str = local_path.to_string_lossy().into_owned();
    let result = run_adb_file_command(serial, &["pull", remote_path, &local_str]);
    // Cihazda iz bırakmamak için geçici dosya temizlenir.
    let _ = run_adb_command(serial, &["shell", "rm", "-f", remote_path]);
    match result {
        Ok(()) => {
            let size = std::fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
            AcquisitionItem {
                category: "screenshot".to_string(),
                file_name: "screenshot.png".to_string(),
                size,
                success: true,
                error: None,
            }
        }
        Err(err) => AcquisitionItem {
            category: "screenshot".to_string(),
            file_name: "screenshot.png".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// /sdcard/Android/media/<package>/ altındaki uygulama medya klasörünü çeker.
fn collect_app_media_dir(
    serial: &str,
    category: &str,
    package: &str,
    target_name: &str,
    dir: &std::path::Path,
) -> AcquisitionItem {
    let remote = format!("/sdcard/Android/media/{package}/");
    let target = dir.join(target_name);
    let _ = std::fs::create_dir_all(&target);
    let target_str = target.to_string_lossy().into_owned();
    match run_adb_file_command(serial, &["pull", &remote, &target_str]) {
        Ok(()) => {
            let size = dir_size(&target);
            AcquisitionItem {
                category: category.to_string(),
                file_name: target_name.to_string(),
                size,
                success: true,
                error: None,
            }
        }
        Err(err) => {
            let size = dir_size(&target);
            if size > 0 {
                AcquisitionItem {
                    category: category.to_string(),
                    file_name: target_name.to_string(),
                    size,
                    success: true,
                    error: Some(format!("Kismi basarili: {err}")),
                }
            } else {
                AcquisitionItem {
                    category: category.to_string(),
                    file_name: target_name.to_string(),
                    size: 0,
                    success: false,
                    error: Some(err),
                }
            }
        }
    }
}

/// WhatsApp medya klasörünü standart uygulama medya yolundan toplar.
fn collect_whatsapp_media(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_app_media_dir(
        serial,
        "whatsapp_media",
        "com.whatsapp",
        "whatsapp_media",
        dir,
    )
}

/// Telegram medya klasörünü standart uygulama medya yolundan toplar.
fn collect_telegram_media(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    collect_app_media_dir(
        serial,
        "telegram_media",
        "org.telegram.messenger",
        "telegram_media",
        dir,
    )
}

/// Bilinen mesajlaşma ve sosyal medya uygulamalarının medya klasörlerini toplar.
fn collect_app_media(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let apps: &[(&str, &str)] = &[
        ("com.whatsapp.w4b", "whatsapp_business"),
        ("com.instagram.android", "instagram"),
        ("com.facebook.orca", "messenger"),
        ("com.facebook.katana", "facebook"),
        ("com.viber.voip", "viber"),
        ("com.google.android.apps.messaging", "google_messages"),
        ("com.twitter.android", "x_twitter"),
        ("com.snapchat.android", "snapchat"),
        ("com.zhiliaoapp.musically", "tiktok"),
        ("com.ss.android.ugc.trill", "tiktok_alt"),
        ("com.discord", "discord"),
        ("com.linkedin.android", "linkedin"),
        ("com.pinterest", "pinterest"),
        ("com.reddit.frontpage", "reddit"),
        ("com.spotify.music", "spotify"),
        ("org.thoughtcrime.securesms", "signal"),
        ("com.skype.raider", "skype"),
        ("us.zoom.videomeetings", "zoom"),
        ("com.microsoft.teams", "teams"),
        ("com.turkcell.bip", "bip"),
        ("com.wire", "wire"),
        ("org.telegram.plus", "telegram_plus"),
        ("com.kakao.talk", "kakaotalk"),
        ("jp.naver.line.android", "line"),
        ("com.tencent.mm", "wechat"),
        ("com.imo.android.imoim", "imo"),
    ];
    let app_media_dir = dir.join("app_media");
    let _ = std::fs::create_dir_all(&app_media_dir);
    let mut total_size = 0_u64;
    let mut found_any = false;

    for (package, folder_name) in apps {
        let remote = format!("/sdcard/Android/media/{package}/");
        let target = app_media_dir.join(folder_name);
        let _ = std::fs::create_dir_all(&target);
        let target_str = target.to_string_lossy().into_owned();
        match run_adb_file_command(serial, &["pull", &remote, &target_str]) {
            Ok(()) => {
                let size = dir_size(&target);
                if size > 0 {
                    found_any = true;
                    total_size += size;
                }
            }
            Err(_) => {
                let size = dir_size(&target);
                if size > 0 {
                    found_any = true;
                    total_size += size;
                } else {
                    let _ = std::fs::remove_dir(&target);
                }
            }
        }
    }

    AcquisitionItem {
        category: "app_media".to_string(),
        file_name: "app_media".to_string(),
        size: total_size,
        success: found_any,
        error: if found_any {
            None
        } else {
            Some("Hicbir uygulama medyasi bulunamadi".to_string())
        },
    }
}

/// /sdcard/Android/media içindeki tüm uygulama klasörlerini dinamik olarak tarar.
fn collect_all_app_media(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let all_media_dir = dir.join("all_app_media");
    let _ = std::fs::create_dir_all(&all_media_dir);

    // Sabit listede olmayan uygulamaları yakalamak için dizin cihazdan okunur.
    let listing = run_adb_command(serial, &["shell", "ls", "/sdcard/Android/media/"]);
    let packages: Vec<String> = match listing {
        Ok(output) => output
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l.contains('.'))
            .collect(),
        Err(err) => {
            return AcquisitionItem {
                category: "all_app_media".to_string(),
                file_name: "all_app_media".to_string(),
                size: 0,
                success: false,
                error: Some(format!("Media dizini listelenemedi: {err}")),
            };
        }
    };

    let mut total_size = 0_u64;
    let mut found_any = false;

    for package in &packages {
        // Paket adı güvenli klasör adına çevrilir.
        let folder_name = package.replace('.', "_");
        let remote = format!("/sdcard/Android/media/{package}/");
        let target = all_media_dir.join(&folder_name);
        let _ = std::fs::create_dir_all(&target);
        let target_str = target.to_string_lossy().into_owned();
        match run_adb_file_command(serial, &["pull", &remote, &target_str]) {
            Ok(()) => {
                let size = dir_size(&target);
                if size > 0 {
                    found_any = true;
                    total_size += size;
                }
            }
            Err(_) => {
                let size = dir_size(&target);
                if size > 0 {
                    found_any = true;
                    total_size += size;
                } else {
                    let _ = std::fs::remove_dir(&target);
                }
            }
        }
    }

    AcquisitionItem {
        category: "all_app_media".to_string(),
        file_name: "all_app_media".to_string(),
        size: total_size,
        success: found_any,
        error: if found_any {
            None
        } else {
            Some("Hicbir ek uygulama medyasi bulunamadi".to_string())
        },
    }
}

/// Android system, secure ve global ayar tablolarını metin olarak toplar.
fn collect_device_settings(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let sections = [
        (
            "=== settings list system ===",
            vec!["shell", "settings", "list", "system"],
        ),
        (
            "=== settings list secure ===",
            vec!["shell", "settings", "list", "secure"],
        ),
        (
            "=== settings list global ===",
            vec!["shell", "settings", "list", "global"],
        ),
    ];
    let mut content = String::new();
    for (header, args) in &sections {
        content.push_str(header);
        content.push('\n');
        match run_adb_command(serial, args.as_slice()) {
            Ok(output) => content.push_str(&output),
            Err(err) => content.push_str(&format!("Hata: {err}")),
        }
        content.push_str("\n\n");
    }
    let path = dir.join("device_settings.txt");
    match std::fs::write(&path, &content) {
        Ok(()) => AcquisitionItem {
            category: "device_settings".to_string(),
            file_name: "device_settings.txt".to_string(),
            size: content.len() as u64,
            success: true,
            error: None,
        },
        Err(err) => AcquisitionItem {
            category: "device_settings".to_string(),
            file_name: "device_settings.txt".to_string(),
            size: 0,
            success: false,
            error: Some(format!("Dosya yazilamadi: {err}")),
        },
    }
}

/// Content provider URI sorgularını dener; izin yoksa hata edinim kaydına yazılır.
fn collect_content_query(
    serial: &str,
    category: &str,
    file_name: &str,
    uri: &str,
    dir: &std::path::Path,
) -> AcquisitionItem {
    collect_shell_output(
        serial,
        category,
        file_name,
        &["shell", "content", "query", "--uri", uri],
        dir,
    )
}

/// Eski Android sürümleri için adb backup çıktısı almaya çalışır.
fn collect_adb_backup(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let target = dir.join("adb_backup.ab");
    let target_str = target.to_string_lossy().into_owned();
    // Kullanıcı uygulamaları ve paylaşımlı alan hedef dosyaya yazdırılır.
    match run_adb_file_command(
        serial,
        &["backup", "-all", "-shared", "-nosystem", "-f", &target_str],
    ) {
        Ok(()) => {
            let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            // Sadece header içeren boş yedekler başarılı kabul edilmez.
            if size > 100 {
                AcquisitionItem {
                    category: "adb_backup".to_string(),
                    file_name: "adb_backup.ab".to_string(),
                    size,
                    success: true,
                    error: None,
                }
            } else {
                AcquisitionItem {
                    category: "adb_backup".to_string(),
                    file_name: "adb_backup.ab".to_string(),
                    size,
                    success: false,
                    error: Some("Backup dosyasi bos veya cihaz tarafindan reddedildi".to_string()),
                }
            }
        }
        Err(err) => AcquisitionItem {
            category: "adb_backup".to_string(),
            file_name: "adb_backup.ab".to_string(),
            size: 0,
            success: false,
            error: Some(err),
        },
    }
}

/// Root, SELinux ve kernel durumunu hızlı doğrulama çıktılarıyla kaydeder.
fn collect_root_status(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let commands: &[(&str, &[&str])] = &[
        ("id", &["shell", "id"]),
        ("su -c id", &["shell", "su", "-c", "id"]),
        ("adb root probe", &["root"]),
        ("getenforce", &["shell", "getenforce"]),
        ("ro.debuggable", &["shell", "getprop", "ro.debuggable"]),
        ("ro.secure", &["shell", "getprop", "ro.secure"]),
        ("kernel", &["shell", "cat", "/proc/version"]),
    ];
    let mut content = String::new();

    for (label, args) in commands {
        content.push_str("=== ");
        content.push_str(label);
        content.push_str(" ===\n");
        match run_adb_command_timeout(serial, args, Duration::from_secs(8)) {
            Ok(output) => content.push_str(output.trim()),
            Err(err) => {
                content.push_str("Hata: ");
                content.push_str(&err);
            }
        }
        content.push_str("\n\n");
    }

    write_text_acquisition_item(dir, "root_status", "root_status.txt", content)
}

/// /proc üzerinden bellek, CPU, mount ve ağ özetlerini toplar.
fn collect_procfs_summary(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let commands: &[(&str, &str)] = &[
        ("meminfo", "cat /proc/meminfo"),
        ("vmstat", "cat /proc/vmstat"),
        ("uptime", "cat /proc/uptime"),
        (
            "pressure memory",
            "cat /proc/pressure/memory 2>/dev/null || true",
        ),
        ("cpuinfo", "cat /proc/cpuinfo"),
        ("mounts", "cat /proc/mounts"),
        ("net tcp", "cat /proc/net/tcp 2>/dev/null || true"),
        ("net tcp6", "cat /proc/net/tcp6 2>/dev/null || true"),
        ("net udp", "cat /proc/net/udp 2>/dev/null || true"),
        ("net unix", "cat /proc/net/unix 2>/dev/null || true"),
    ];
    let mut content = String::new();

    for (label, shell) in commands {
        content.push_str("=== /proc ");
        content.push_str(label);
        content.push_str(" ===\n");
        match run_adb_command_timeout(
            serial,
            &["shell", "sh", "-c", shell],
            Duration::from_secs(10),
        ) {
            Ok(output) => content.push_str(output.trim()),
            Err(err) => {
                content.push_str("Hata: ");
                content.push_str(&err);
            }
        }
        content.push_str("\n\n");
    }

    write_text_acquisition_item(dir, "procfs_summary", "procfs_summary.txt", content)
}

/// Erişilebilen proseslerin /proc/<pid>/maps kayıtlarını dosyalara ayırır.
fn collect_proc_memory_maps(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let target_dir = dir.join("proc_memory_maps");
    let _ = std::fs::create_dir_all(&target_dir);
    let ps_output =
        match run_adb_command_timeout(serial, &["shell", "ps", "-A"], Duration::from_secs(20)) {
            Ok(output) => output,
            Err(err) => {
                return AcquisitionItem {
                    category: "proc_memory_maps".to_string(),
                    file_name: "proc_memory_maps".to_string(),
                    size: 0,
                    success: false,
                    error: Some(format!("Process listesi alinamadi: {err}")),
                };
            }
        };

    let processes = parse_process_rows(&ps_output);
    let mut index = String::new();
    let mut captured = 0_usize;
    let mut failed = 0_usize;

    for (pid, name) in processes.into_iter().take(80) {
        let shell = format!("cat /proc/{pid}/maps 2>&1 | head -n 2000");
        let output = run_adb_command_timeout(
            serial,
            &["shell", "sh", "-c", &shell],
            Duration::from_secs(4),
        );
        match output {
            Ok(maps) if maps.contains('-') && !maps.contains("Permission denied") => {
                let safe_name = sanitize_file_component(&name);
                let file_name = format!("{pid}_{safe_name}.maps");
                let path = target_dir.join(&file_name);
                if std::fs::write(&path, &maps).is_ok() {
                    captured += 1;
                    index.push_str(&format!("{pid}\t{name}\t{file_name}\n"));
                } else {
                    failed += 1;
                    index.push_str(&format!("{pid}\t{name}\twrite_failed\n"));
                }
            }
            Ok(output) => {
                failed += 1;
                let detail = first_non_empty(&output).unwrap_or_else(|| "empty".to_string());
                index.push_str(&format!("{pid}\t{name}\t{detail}\n"));
            }
            Err(err) => {
                failed += 1;
                index.push_str(&format!("{pid}\t{name}\t{err}\n"));
            }
        }
    }

    let _ = std::fs::write(
        target_dir.join("index.tsv"),
        format!("pid\tname\tmaps_file_or_error\n{index}"),
    );
    let size = dir_size(&target_dir);
    AcquisitionItem {
        category: "proc_memory_maps".to_string(),
        file_name: "proc_memory_maps".to_string(),
        size,
        success: captured > 0,
        error: if captured > 0 {
            Some(format!("{captured} process maps alindi, {failed} atlandi"))
        } else {
            Some(format!("Process maps okunamadi, {failed} process atlandi"))
        },
    }
}

/// Debuggable paketleri bulup olası heap dump hedeflerini listeler.
fn collect_heapdump_candidates(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let dumpsys = match run_adb_command_timeout(
        serial,
        &["shell", "dumpsys", "package"],
        Duration::from_secs(90),
    ) {
        Ok(output) => output,
        Err(err) => {
            return AcquisitionItem {
                category: "heapdump_candidates".to_string(),
                file_name: "heapdump_candidates.txt".to_string(),
                size: 0,
                success: false,
                error: Some(format!("Paket bilgisi alinamadi: {err}")),
            };
        }
    };

    let packages = parse_debuggable_packages(&dumpsys);
    let mut content = String::new();
    content.push_str("# Debuggable package candidates for adb shell am dumpheap\n");
    content.push_str("# Full process memory still requires root or ptrace privileges.\n\n");
    for package in &packages {
        content.push_str("package=");
        content.push_str(package);
        content.push('\n');
    }
    if packages.is_empty() {
        content.push_str("No debuggable package was detected from dumpsys package.\n");
    }

    let mut item = write_text_acquisition_item(
        dir,
        "heapdump_candidates",
        "heapdump_candidates.txt",
        content,
    );
    item.success = true;
    item.error = Some(format!("{} aday bulundu", packages.len()));
    item
}

/// Debuggable ve çalışan uygulamalar için HPROF heap dump toplamayı dener.
fn collect_debug_heap_dumps(serial: &str, dir: &std::path::Path) -> AcquisitionItem {
    let candidates_path = dir.join("heapdump_candidates.txt");
    let packages = std::fs::read_to_string(&candidates_path)
        .ok()
        .map(|content| parse_candidate_package_lines(&content))
        .filter(|packages| !packages.is_empty())
        .unwrap_or_else(|| {
            run_adb_command_timeout(
                serial,
                &["shell", "dumpsys", "package"],
                Duration::from_secs(90),
            )
            .map(|output| parse_debuggable_packages(&output))
            .unwrap_or_default()
        });

    let target_dir = dir.join("debug_heap_dumps");
    let _ = std::fs::create_dir_all(&target_dir);
    let mut log = String::new();
    let mut dumped = 0_usize;
    let mut failed = 0_usize;

    for package in packages.iter().take(5) {
        let pid_output =
            run_adb_command_timeout(serial, &["shell", "pidof", package], Duration::from_secs(5));
        let pid = match pid_output
            .ok()
            .and_then(|output| output.split_whitespace().next().map(ToOwned::to_owned))
            .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        {
            Some(pid) => pid,
            None => {
                failed += 1;
                log.push_str(&format!("{package}\tpid_not_running\n"));
                continue;
            }
        };

        let safe_package = sanitize_file_component(package);
        let remote_path = format!("/sdcard/Download/amele_heap_{safe_package}_{pid}.hprof");
        let local_file = format!("{safe_package}_{pid}.hprof");
        let local_path = target_dir.join(&local_file);
        let local_arg = local_path.to_string_lossy().into_owned();

        match run_adb_command_timeout(
            serial,
            &["shell", "am", "dumpheap", &pid, &remote_path],
            Duration::from_secs(60),
        ) {
            Ok(_) => {}
            Err(err) => {
                failed += 1;
                log.push_str(&format!("{package}\t{pid}\tdumpheap_failed\t{err}\n"));
                let _ = run_adb_command_timeout(
                    serial,
                    &["shell", "rm", "-f", &remote_path],
                    Duration::from_secs(5),
                );
                continue;
            }
        }

        match run_adb_file_command_timeout(
            serial,
            &["pull", &remote_path, &local_arg],
            Duration::from_secs(120),
        ) {
            Ok(()) => {
                let size = std::fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
                if size > 0 {
                    dumped += 1;
                    log.push_str(&format!("{package}\t{pid}\t{local_file}\t{size}\n"));
                } else {
                    failed += 1;
                    log.push_str(&format!("{package}\t{pid}\tempty_hprof\n"));
                }
            }
            Err(err) => {
                failed += 1;
                log.push_str(&format!("{package}\t{pid}\tpull_failed\t{err}\n"));
            }
        }

        let _ = run_adb_command_timeout(
            serial,
            &["shell", "rm", "-f", &remote_path],
            Duration::from_secs(5),
        );
    }

    let _ = std::fs::write(
        target_dir.join("heapdump_log.tsv"),
        format!("package\tpid\tfile_or_status\tsize_or_error\n{log}"),
    );
    let size = dir_size(&target_dir);
    AcquisitionItem {
        category: "debug_heap_dumps".to_string(),
        file_name: "debug_heap_dumps".to_string(),
        size,
        success: dumped > 0,
        error: if dumped > 0 {
            Some(format!("{dumped} HPROF alindi, {failed} hedef atlandi"))
        } else {
            Some("HPROF alinamadi; debuggable ve calisan uygulama gerekir".to_string())
        },
    }
}

/// Metin çıktısını dosyaya yazar ve ortak AcquisitionItem formatına çevirir.
fn write_text_acquisition_item(
    dir: &std::path::Path,
    category: &str,
    file_name: &str,
    content: String,
) -> AcquisitionItem {
    let path = dir.join(file_name);
    match std::fs::write(&path, &content) {
        Ok(()) => AcquisitionItem {
            category: category.to_string(),
            file_name: file_name.to_string(),
            size: content.len() as u64,
            success: true,
            error: None,
        },
        Err(err) => AcquisitionItem {
            category: category.to_string(),
            file_name: file_name.to_string(),
            size: 0,
            success: false,
            error: Some(format!("Dosya yazilamadi: {err}")),
        },
    }
}

/// JSON çıktısını dosyaya yazar ve ortak AcquisitionItem formatına çevirir.
fn write_json_acquisition_item<T: Serialize>(
    dir: &std::path::Path,
    category: &str,
    file_name: &str,
    value: &T,
) -> AcquisitionItem {
    match serde_json::to_vec_pretty(value)
        .map_err(|err| err.to_string())
        .and_then(|content| {
            let path = dir.join(file_name);
            std::fs::write(&path, &content)
                .map(|_| content.len() as u64)
                .map_err(|err| err.to_string())
        }) {
        Ok(size) => AcquisitionItem {
            category: category.to_string(),
            file_name: file_name.to_string(),
            size,
            success: true,
            error: None,
        },
        Err(err) => AcquisitionItem {
            category: category.to_string(),
            file_name: file_name.to_string(),
            size: 0,
            success: false,
            error: Some(format!("JSON yazilamadi: {err}")),
        },
    }
}

/// dumpsys package çıktısından debug izni açık paketleri ayıklar.
fn parse_debuggable_packages(dumpsys_package: &str) -> Vec<String> {
    let mut current_package: Option<String> = None;
    let mut packages = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in dumpsys_package.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Package [") {
            current_package = rest.split(']').next().map(ToOwned::to_owned);
            continue;
        }
        let debuggable = trimmed.contains("DEBUGGABLE")
            || trimmed.contains("FLAG_DEBUGGABLE")
            || trimmed.contains("debuggable=true");
        if debuggable {
            if let Some(package) = &current_package {
                if seen.insert(package.clone()) {
                    packages.push(package.clone());
                }
            }
        }
    }

    packages
}

/// Heap dump aday dosyasındaki package satırlarını paket listesine çevirir.
fn parse_candidate_package_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package="))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// ps -A çıktısını PID ve proses adı çiftlerine indirger.
fn parse_process_rows(ps_output: &str) -> Vec<(u32, String)> {
    ps_output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let pid_index = parts.iter().position(|part| part.parse::<u32>().is_ok())?;
            let pid = parts.get(pid_index)?.parse::<u32>().ok()?;
            let name = parts.last().copied().unwrap_or("process").to_string();
            Some((pid, name))
        })
        .collect()
}

/// Dosya/klasör adında sorun çıkarabilecek karakterleri güvenli hale getirir.
fn sanitize_file_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

/// Klasör ağacındaki dosyaların toplam boyutunu hesaplar.
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0_u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total += dir_size(&entry_path);
            } else {
                total += std::fs::metadata(&entry_path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// Edinim sonunda manifest.json dosyasını hash ve adım sonuçlarıyla üretir.
fn write_manifest(
    dir: &std::path::Path,
    items: &[AcquisitionItem],
    serial: &str,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let manifest = serde_json::json!({
        "serial": serial,
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "items": items,
    });
    let content = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("Manifest olusturulamadi: {err}"))?;

    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, &content)
        .map_err(|err| format!("Manifest yazilamadi: {err}"))?;

    let hash = crate::hash::to_hex(&Sha256::digest(content.as_bytes()));
    let sidecar = dir.join("manifest.json.sha256");
    let _ = std::fs::write(&sidecar, format!("{hash}  manifest.json\n"));
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_debuggable_packages_from_dumpsys() {
        let packages = parse_debuggable_packages(
            "Package [com.example.normal] (abc):\n\
             pkgFlags=[ HAS_CODE ALLOW_CLEAR_USER_DATA ]\n\
             Package [com.example.debug] (def):\n\
             pkgFlags=[ HAS_CODE DEBUGGABLE ALLOW_CLEAR_USER_DATA ]\n",
        );

        assert_eq!(packages, vec!["com.example.debug"]);
    }

    #[test]
    fn parses_process_rows_from_ps_output() {
        let rows = parse_process_rows(
            "USER PID PPID VSZ RSS WCHAN ADDR S NAME\n\
             u0_a123 2345 123 100 20 0 0 S com.example.app\n\
             root 1 0 0 0 0 0 S init\n",
        );

        assert_eq!(rows[0], (2345, "com.example.app".to_string()));
        assert_eq!(rows[1], (1, "init".to_string()));
    }

    #[test]
    fn parses_package_rows_for_json_manifest() {
        let packages = parse_package_rows(
            "package:/data/app/~~abc/base.apk=com.example.app uid:10123 versionCode:42\n\
             package:/system/app/Settings/Settings.apk=com.android.settings\n",
        );

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package, "com.example.app");
        assert_eq!(
            packages[0].apk_path.as_deref(),
            Some("/data/app/~~abc/base.apk")
        );
        assert_eq!(packages[0].uid.as_deref(), Some("10123"));
        assert_eq!(packages[0].version_code.as_deref(), Some("42"));
        assert_eq!(packages[1].package, "com.android.settings");
    }

    #[test]
    fn turkey_catalog_contains_common_targets() {
        let packages: Vec<&str> = ANDROID_APP_TARGETS
            .iter()
            .filter(|target| target.priority == "turkey_high")
            .map(|target| target.package)
            .collect();

        assert!(packages.contains(&"trendyol.com"));
        assert!(packages.contains(&"com.getir"));
        assert!(packages.contains(&"com.sahibinden"));
        assert!(packages.contains(&"com.inomera.sm"));
        assert!(packages.contains(&"com.pozitron.iscep"));
        assert!(packages.contains(&"com.akbank.android.apps.akbank_direkt"));
        assert!(packages.contains(&"com.ykb.android"));
        assert!(packages.contains(&"com.tmobtech.halkbank"));
    }

    #[test]
    fn parses_storage_probe_output() {
        let probe = parse_storage_probe_output(
            "/sdcard/Android/media/com.whatsapp",
            "__EXISTS__\n128\t/sdcard/Android/media/com.whatsapp\n/sdcard/Android/media/com.whatsapp/WhatsApp/Media/a.jpg\n",
        );

        assert!(probe.exists);
        assert_eq!(probe.size_kb, Some(128));
        assert_eq!(probe.sample_files.len(), 1);
    }
}

/// Varsayılan profil ile Android mantıksal edinim akışını başlatır.
pub fn logical_acquisition<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    progress: F,
    cancelled: C,
) -> Result<LogicalAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    logical_acquisition_with_profile(
        serial,
        output_dir,
        AndroidAcquisitionProfile::FullLogical,
        progress,
        cancelled,
    )
}

/// Seçilen profile göre Android mantıksal edinim adımlarını sırayla çalıştırır.
pub fn logical_acquisition_with_profile<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    profile: AndroidAcquisitionProfile,
    mut progress: F,
    cancelled: C,
) -> Result<LogicalAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    std::fs::create_dir_all(output_dir)
        .map_err(|err| format!("Cikti dizini olusturulamadi: {err}"))?;

    let steps = logical_steps_for_profile(profile);
    let total = (steps.len() + 2) as u32;
    let mut items = Vec::with_capacity(steps.len() + 1);
    let mut errors = Vec::new();

    for (step_index, step) in steps.iter().enumerate() {
        let category = step.category;
        if cancelled() {
            errors.push("Kullanici tarafindan iptal edildi".to_string());
            break;
        }

        progress(step_index as u32, total, category);

        let item = match category {
            "device_info" => collect_device_info(serial, output_dir),
            "packages" => collect_packages(serial, output_dir),
            "packages_json" => collect_packages_json(serial, output_dir),
            "social_apps" => collect_social_apps(serial, output_dir),
            "turkey_app_storage" => collect_turkey_app_storage(serial, output_dir),
            "logcat" => collect_logcat(serial, output_dir),
            "system_logs" => collect_system_logs(serial, output_dir),
            "dumpsys_battery" => {
                collect_dumpsys(serial, "battery", "dumpsys_battery.txt", output_dir)
            }
            "dumpsys_wifi" => collect_dumpsys(serial, "wifi", "dumpsys_wifi.txt", output_dir),
            "dumpsys_bluetooth" => collect_dumpsys(
                serial,
                "bluetooth_manager",
                "dumpsys_bluetooth.txt",
                output_dir,
            ),
            "dumpsys_usagestats" => {
                collect_dumpsys(serial, "usagestats", "dumpsys_usagestats.txt", output_dir)
            }
            "dumpsys_account" => {
                collect_dumpsys(serial, "account", "dumpsys_account.txt", output_dir)
            }
            "social_accounts" => collect_social_accounts(serial, output_dir),
            "dumpsys_connectivity" => collect_dumpsys(
                serial,
                "connectivity",
                "dumpsys_connectivity.txt",
                output_dir,
            ),
            "dumpsys_notification" => collect_notification_history(serial, output_dir),
            "dumpsys_telephony" => collect_dumpsys(
                serial,
                "telephony.registry",
                "dumpsys_telephony.txt",
                output_dir,
            ),
            "dumpsys_location" => {
                collect_dumpsys(serial, "location", "dumpsys_location.txt", output_dir)
            }
            "dumpsys_netstats" => {
                collect_dumpsys(serial, "netstats", "dumpsys_netstats.txt", output_dir)
            }
            "dumpsys_activity" => {
                collect_dumpsys(serial, "activity", "dumpsys_activity.txt", output_dir)
            }
            "dumpsys_meminfo" => {
                collect_dumpsys(serial, "meminfo", "dumpsys_meminfo.txt", output_dir)
            }
            "dumpsys_appops" => collect_dumpsys(serial, "appops", "dumpsys_appops.txt", output_dir),
            "dumpsys_package" => {
                collect_dumpsys(serial, "package", "dumpsys_package.txt", output_dir)
            }
            "dumpsys_diskstats" => {
                collect_dumpsys(serial, "diskstats", "dumpsys_diskstats.txt", output_dir)
            }
            "dumpsys_deviceidle" => {
                collect_dumpsys(serial, "deviceidle", "dumpsys_deviceidle.txt", output_dir)
            }
            "dumpsys_alarm" => collect_dumpsys(serial, "alarm", "dumpsys_alarm.txt", output_dir),
            "dumpsys_jobscheduler" => collect_dumpsys(
                serial,
                "jobscheduler",
                "dumpsys_jobscheduler.txt",
                output_dir,
            ),
            "dumpsys_procstats" => {
                collect_dumpsys(serial, "procstats", "dumpsys_procstats.txt", output_dir)
            }
            "dumpsys_sensorservice" => collect_dumpsys(
                serial,
                "sensorservice",
                "dumpsys_sensorservice.txt",
                output_dir,
            ),
            "dumpsys_power" => collect_dumpsys(serial, "power", "dumpsys_power.txt", output_dir),
            "dumpsys_window" => collect_dumpsys(serial, "window", "dumpsys_window.txt", output_dir),
            "dumpsys_clipboard" => {
                collect_dumpsys(serial, "clipboard", "dumpsys_clipboard.txt", output_dir)
            }
            "dumpsys_batterystats" => collect_dumpsys(
                serial,
                "batterystats",
                "dumpsys_batterystats.txt",
                output_dir,
            ),
            "dumpsys_keystore" => {
                collect_dumpsys(serial, "keystore", "dumpsys_keystore.txt", output_dir)
            }
            "root_status" => collect_root_status(serial, output_dir),
            "root_binaries" => collect_root_binaries(serial, output_dir),
            "selinux_status" => collect_selinux_status(serial, output_dir),
            "services" => collect_services(serial, output_dir),
            "mounts" => collect_mounts(serial, output_dir),
            "environment" => collect_environment(serial, output_dir),
            "temp_files" => collect_temp_files(serial, output_dir),
            "intrusion_indicators" => collect_intrusion_indicators(serial, output_dir),
            "file_index" => collect_file_index(serial, output_dir),
            "procfs_summary" => collect_procfs_summary(serial, output_dir),
            "proc_memory_maps" => collect_proc_memory_maps(serial, output_dir),
            "heapdump_candidates" => collect_heapdump_candidates(serial, output_dir),
            "debug_heap_dumps" => collect_debug_heap_dumps(serial, output_dir),
            "device_settings" => collect_device_settings(serial, output_dir),
            "network_info" => collect_network_info(serial, output_dir),
            "processes" => collect_processes(serial, output_dir),
            "social_processes" => collect_social_processes(serial, output_dir),
            "disk_usage" => collect_disk_usage(serial, output_dir),
            "content_sms" => collect_content_query(
                serial,
                "content_sms",
                "content_sms.txt",
                "content://sms",
                output_dir,
            ),
            "content_calls" => collect_content_query(
                serial,
                "content_calls",
                "content_calls.txt",
                "content://call_log/calls",
                output_dir,
            ),
            "content_contacts" => collect_content_query(
                serial,
                "content_contacts",
                "content_contacts.txt",
                "content://contacts/phones",
                output_dir,
            ),
            "content_user_dictionary" => collect_content_query(
                serial,
                "content_user_dictionary",
                "content_user_dictionary.txt",
                "content://user_dictionary/words",
                output_dir,
            ),
            "content_calendar" => collect_content_query(
                serial,
                "content_calendar",
                "content_calendar.txt",
                "content://com.android.calendar/events",
                output_dir,
            ),
            "content_media_images" => collect_content_query(
                serial,
                "content_media_images",
                "content_media_images.txt",
                "content://media/external/images/media",
                output_dir,
            ),
            "content_media_videos" => collect_content_query(
                serial,
                "content_media_videos",
                "content_media_videos.txt",
                "content://media/external/video/media",
                output_dir,
            ),
            "content_media_audio" => collect_content_query(
                serial,
                "content_media_audio",
                "content_media_audio.txt",
                "content://media/external/audio/media",
                output_dir,
            ),
            "content_media_files" => collect_content_query(
                serial,
                "content_media_files",
                "content_media_files.txt",
                "content://media/external/file",
                output_dir,
            ),
            "content_telephony_carriers" => collect_content_query(
                serial,
                "content_telephony_carriers",
                "content_telephony_carriers.txt",
                "content://telephony/carriers",
                output_dir,
            ),
            "screenshot" => collect_screenshot(serial, output_dir),
            "whatsapp_media" => collect_whatsapp_media(serial, output_dir),
            "telegram_media" => collect_telegram_media(serial, output_dir),
            "app_media" => collect_app_media(serial, output_dir),
            "all_app_media" => collect_all_app_media(serial, output_dir),
            "adb_backup" => collect_adb_backup(serial, output_dir),
            "bugreport" => collect_bugreport(serial, output_dir),
            "shared_storage" => collect_shared_storage(serial, output_dir),
            _ => continue,
        };

        if !item.success {
            if let Some(err) = &item.error {
                errors.push(format!("{category}: {err}"));
            }
        }
        items.push(item);
    }

    progress(steps.len() as u32, total, "mft_archive");
    match crate::android_mft::write_logical_mft_bundle(serial, output_dir) {
        Ok(bundle) => {
            items.push(AcquisitionItem {
                category: "mft_archive".to_string(),
                file_name: bundle.file_name.clone(),
                size: bundle.size,
                success: true,
                error: None,
            });
        }
        Err(err) => {
            errors.push(format!("mft_archive: {err}"));
            items.push(AcquisitionItem {
                category: "mft_archive".to_string(),
                file_name: "evidence.mft".to_string(),
                size: 0,
                success: false,
                error: Some(err),
            });
        }
    }

    progress(steps.len() as u32 + 1, total, "analysis_outputs");
    match crate::android_mft::write_logical_analysis_outputs(serial, output_dir) {
        Ok(outputs) => {
            for output in outputs {
                items.push(AcquisitionItem {
                    category: "analysis_output".to_string(),
                    file_name: output.file_name,
                    size: output.size,
                    success: true,
                    error: None,
                });
            }
        }
        Err(err) => {
            errors.push(format!("analysis_outputs: {err}"));
            items.push(AcquisitionItem {
                category: "analysis_output".to_string(),
                file_name: "analysis_outputs".to_string(),
                size: 0,
                success: false,
                error: Some(err),
            });
        }
    }

    // Final progress tick
    progress(total, total, "manifest");

    let total_bytes = items.iter().map(|i| i.size).sum();
    let sha256 = write_manifest(output_dir, &items, serial).ok();

    Ok(LogicalAcquisitionResult {
        output_dir: output_dir.to_path_buf(),
        items,
        total_bytes,
        sha256,
        errors,
    })
}
