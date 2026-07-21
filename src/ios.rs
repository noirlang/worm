//! iOS iTunes/Finder backup normalizasyonu.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::hash::{HashAlgorithm, calculate_file_hash, calculate_multiple};
use crate::logging::{LogLevel, runtime_log};
use chrono::Local;
use plist::Value as PlistValue;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const COPY_BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// iOS backup klasöründen hızlıca okunabilen cihaz ve backup profilidir.
pub struct IosBackupInfo {
    pub backup_path: PathBuf,
    pub manifest_db: bool,
    pub manifest_plist: bool,
    pub info_plist: bool,
    pub status_plist: bool,
    pub encrypted: bool,
    pub file_count: Option<u64>,
    pub device_name: Option<String>,
    pub model: Option<String>,
    pub product_type: Option<String>,
    pub ios_version: Option<String>,
    pub build_version: Option<String>,
    pub serial_number: Option<String>,
    pub unique_device_id: Option<String>,
    pub imei: Option<String>,
    pub meid: Option<String>,
    pub iccid: Option<String>,
    pub phone_number: Option<String>,
    pub last_backup_date: Option<String>,
    pub installed_apps_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// iOS backup normalizasyonu tamamlandığında UI/API tarafına dönen özet.
pub struct IosNormalizeResult {
    pub output_dir: PathBuf,
    pub log_path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_sha256: Option<String>,
    pub total_entries: u64,
    pub files_copied: u64,
    pub directories: u64,
    pub symlinks: u64,
    pub missing: u64,
    pub errors: u64,
    pub total_bytes: u64,
    pub encrypted: bool,
}

#[derive(Default)]
struct NormalizeCounters {
    total_entries: u64,
    processed: u64,
    files_copied: u64,
    directories: u64,
    symlinks: u64,
    missing: u64,
    errors: u64,
    total_bytes: u64,
}

#[derive(Debug)]
struct BackupEntry {
    file_id: String,
    domain: String,
    relative_path: String,
    flags: i64,
}

/// Backup klasörünün temel geçerliliğini ve cihaz profilini okur.
pub fn inspect_backup(backup_path: impl AsRef<Path>) -> AmeleResult<IosBackupInfo> {
    let backup_path = backup_path.as_ref();
    if !backup_path.is_dir() {
        return Err(AmeleError::new(
            HataKodu::DosyaAcilamadi,
            format!("iOS backup klasoru bulunamadi: {}", backup_path.display()),
        ));
    }

    let manifest_db_path = backup_path.join("Manifest.db");
    let manifest_plist_path = backup_path.join("Manifest.plist");
    let info_plist_path = backup_path.join("Info.plist");
    let status_plist_path = backup_path.join("Status.plist");
    let lowercase_status_plist_path = backup_path.join("status.plist");

    let manifest = read_plist_dictionary(&manifest_plist_path);
    let info = read_plist_dictionary(&info_plist_path);
    let status = read_plist_dictionary(&status_plist_path)
        .or_else(|| read_plist_dictionary(&lowercase_status_plist_path));

    let lockdown = manifest
        .as_ref()
        .and_then(|dict| dict.get("Lockdown"))
        .and_then(PlistValue::as_dictionary);
    let encrypted = manifest
        .as_ref()
        .and_then(|dict| dict.get("IsEncrypted"))
        .and_then(PlistValue::as_boolean)
        .unwrap_or(false);

    let file_count = if manifest_db_path.is_file() && !encrypted {
        count_manifest_entries(&manifest_db_path).ok()
    } else {
        None
    };

    Ok(IosBackupInfo {
        backup_path: backup_path.to_path_buf(),
        manifest_db: manifest_db_path.is_file(),
        manifest_plist: manifest_plist_path.is_file(),
        info_plist: info_plist_path.is_file(),
        status_plist: status_plist_path.is_file() || lowercase_status_plist_path.is_file(),
        encrypted,
        file_count,
        device_name: plist_string(lockdown, "DeviceName")
            .or_else(|| plist_string(info.as_ref(), "Device Name")),
        model: plist_string(lockdown, "ProductType")
            .map(|value| ios_model_name(&value).to_string()),
        product_type: plist_string(lockdown, "ProductType"),
        ios_version: plist_string(lockdown, "ProductVersion"),
        build_version: plist_string(lockdown, "BuildVersion"),
        serial_number: plist_string(lockdown, "SerialNumber")
            .or_else(|| plist_string(info.as_ref(), "Serial Number")),
        unique_device_id: plist_string(lockdown, "UniqueDeviceID")
            .or_else(|| plist_string(info.as_ref(), "Unique Identifier")),
        imei: plist_string(info.as_ref(), "IMEI"),
        meid: plist_string(info.as_ref(), "MEID"),
        iccid: plist_string(info.as_ref(), "ICCID"),
        phone_number: plist_string(info.as_ref(), "Phone Number"),
        last_backup_date: plist_string(info.as_ref(), "Last Backup Date")
            .or_else(|| plist_string(status.as_ref(), "Date")),
        installed_apps_count: installed_apps_count(info.as_ref()),
    })
}

/// iOS backup dosyalarını Manifest.db indeksine göre okunabilir dosya ağacına çıkarır.
pub fn normalize_backup<Progress, Log, Paused, Cancelled>(
    backup_path: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    hash_algorithms: &[HashAlgorithm],
    mut progress: Progress,
    mut log: Log,
    mut is_paused: Paused,
    mut is_cancelled: Cancelled,
) -> AmeleResult<IosNormalizeResult>
where
    Progress: FnMut(u64, u64, &str),
    Log: FnMut(&str),
    Paused: FnMut() -> bool,
    Cancelled: FnMut() -> bool,
{
    let backup_path = backup_path.as_ref();
    let output_dir = output_dir.as_ref();
    let info = inspect_backup(backup_path)?;

    if !info.manifest_db {
        return Err(AmeleError::new(
            HataKodu::DosyaAcilamadi,
            "Manifest.db bulunamadi; secilen klasor gecerli bir iOS backup klasoru degil",
        ));
    }
    if info.encrypted {
        return Err(AmeleError::new(
            HataKodu::IcerikGecersiz,
            "Sifreli iOS backup algilandi. Bu native normalizer su anda sifresi kaldirilmis/decrypted backup klasorlerini isler.",
        ));
    }

    fs::create_dir_all(output_dir).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!("iOS cikti klasoru olusturulamadi: {}", output_dir.display()),
            err,
        )
    })?;

    let manifest_db = backup_path.join("Manifest.db");
    let log_path = output_dir.join(format!(
        "extraction_log_{}.csv",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    init_csv_log(&log_path)?;
    log(&format!(
        "iOS backup normalizasyonu basladi: {}",
        backup_path.display()
    ));
    log(&format!("Cikti klasoru: {}", output_dir.display()));

    let connection = Connection::open_with_flags(
        &manifest_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| {
        AmeleError::new(
            HataKodu::IcerikGecersiz,
            format!("Manifest.db okunamadi: {err}"),
        )
    })?;

    let mut counters = NormalizeCounters {
        total_entries: query_total_entries(&connection)?,
        ..NormalizeCounters::default()
    };
    progress(0, counters.total_entries, "iOS backup indeksi okundu");

    let mut statement = connection
        .prepare(
            "SELECT fileID, domain, relativePath, flags FROM Files ORDER BY domain, relativePath",
        )
        .map_err(|err| {
            AmeleError::new(
                HataKodu::IcerikGecersiz,
                format!("Manifest sorgusu hazirlanamadi: {err}"),
            )
        })?;
    let entries = statement
        .query_map([], |row| {
            Ok(BackupEntry {
                file_id: row.get::<_, String>(0)?,
                domain: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                relative_path: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                flags: row.get::<_, Option<i64>>(3)?.unwrap_or(1),
            })
        })
        .map_err(|err| {
            AmeleError::new(
                HataKodu::IcerikGecersiz,
                format!("Manifest satirlari okunamadi: {err}"),
            )
        })?;

    for entry in entries {
        if is_cancelled() {
            return Err(AmeleError::new(
                HataKodu::Genel,
                "iOS backup normalizasyonu durduruldu",
            ));
        }
        while is_paused() {
            if is_cancelled() {
                return Err(AmeleError::new(
                    HataKodu::Genel,
                    "iOS backup normalizasyonu durduruldu",
                ));
            }
            thread::sleep(Duration::from_millis(160));
        }

        let entry = entry.map_err(|err| {
            AmeleError::new(
                HataKodu::IcerikGecersiz,
                format!("Manifest girdisi okunamadi: {err}"),
            )
        })?;
        counters.processed += 1;

        process_entry(
            backup_path,
            output_dir,
            &log_path,
            hash_algorithms,
            &entry,
            &mut counters,
        );

        if counters.processed % 10 == 0 || counters.processed == counters.total_entries {
            let msg = format!(
                "Islenen kayit: {}/{}",
                counters.processed, counters.total_entries
            );
            progress(counters.processed, counters.total_entries, &msg);
            log(&msg);
        }
    }

    let manifest_path = output_dir.join("ios_manifest.json");
    write_ios_manifest(&manifest_path, &info, &counters, &log_path, output_dir)?;
    let manifest_sha256 = calculate_file_hash(&manifest_path, HashAlgorithm::Sha256).ok();
    if let Some(hash) = &manifest_sha256 {
        let sidecar = output_dir.join("ios_manifest.json.sha256");
        let _ = fs::write(&sidecar, format!("{hash}  ios_manifest.json\n"));
    }

    log(&format!(
        "iOS backup normalizasyonu tamamlandi: {} dosya, {} klasor, {} eksik, {} hata",
        counters.files_copied, counters.directories, counters.missing, counters.errors
    ));

    Ok(IosNormalizeResult {
        output_dir: output_dir.to_path_buf(),
        log_path,
        manifest_path,
        manifest_sha256,
        total_entries: counters.total_entries,
        files_copied: counters.files_copied,
        directories: counters.directories,
        symlinks: counters.symlinks,
        missing: counters.missing,
        errors: counters.errors,
        total_bytes: counters.total_bytes,
        encrypted: info.encrypted,
    })
}

fn process_entry(
    backup_path: &Path,
    output_dir: &Path,
    log_path: &Path,
    hash_algorithms: &[HashAlgorithm],
    entry: &BackupEntry,
    counters: &mut NormalizeCounters,
) {
    let destination = map_ios_path(output_dir, &entry.domain, &entry.relative_path);

    if entry.flags == 2 {
        match fs::create_dir_all(&destination) {
            Ok(()) => {
                counters.directories += 1;
                let _ =
                    append_csv_log(log_path, "Directory", entry, Some(&destination), None, None);
            }
            Err(_) => {
                counters.errors += 1;
                let _ = append_csv_log(log_path, "Error", entry, Some(&destination), None, None);
            }
        }
        return;
    }

    if entry.flags == 4 {
        counters.symlinks += 1;
        let _ = append_csv_log(log_path, "Symlink", entry, None, None, None);
        return;
    }

    let Some(source) = backup_file_path(backup_path, &entry.file_id) else {
        counters.errors += 1;
        let _ = append_csv_log(log_path, "Error", entry, None, None, None);
        return;
    };

    if !source.is_file() {
        counters.missing += 1;
        let _ = append_csv_log(log_path, "Missing", entry, None, None, None);
        return;
    }

    match copy_backup_file(&source, &destination, hash_algorithms) {
        Ok((bytes, hashes)) => {
            counters.files_copied += 1;
            counters.total_bytes = counters.total_bytes.saturating_add(bytes);
            let _ = append_csv_log(
                log_path,
                "Copied",
                entry,
                Some(&destination),
                Some(bytes),
                Some(&hashes),
            );
        }
        Err(err) => {
            runtime_log(
                LogLevel::Warn,
                "ios",
                format!(
                    "iOS backup dosyasi kopyalanamadi: {} -> {} | {}",
                    source.display(),
                    destination.display(),
                    err
                ),
            );
            counters.errors += 1;
            let _ = append_csv_log(log_path, "Error", entry, Some(&destination), None, None);
        }
    }
}

fn copy_backup_file(
    source: &Path,
    destination: &Path,
    hash_algorithms: &[HashAlgorithm],
) -> AmeleResult<(u64, Vec<(HashAlgorithm, String)>)> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                format!("Hedef klasor olusturulamadi: {}", parent.display()),
                err,
            )
        })?;
    }

    let bytes = stream_copy(source, destination)?;
    let hashes = if hash_algorithms.is_empty() {
        Vec::new()
    } else {
        calculate_multiple(destination, hash_algorithms)?
            .into_iter()
            .map(|item| (item.algorithm, item.value))
            .collect()
    };
    Ok((bytes, hashes))
}

fn stream_copy(source: &Path, destination: &Path) -> AmeleResult<u64> {
    let mut input = File::open(source).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaAcilamadi,
            format!("Kaynak backup dosyasi acilamadi: {}", source.display()),
            err,
        )
    })?;
    let mut output = File::create(destination).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!("Hedef dosya olusturulamadi: {}", destination.display()),
            err,
        )
    })?;

    let mut total = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    loop {
        let read = input.read(&mut buffer).map_err(|err| {
            AmeleError::io(HataKodu::DosyaOkuma, "iOS backup dosyasi okunamadi", err)
        })?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "iOS hedef dosyasi yazilamadi", err)
        })?;
        total = total.saturating_add(read as u64);
    }
    output.flush().map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            "iOS hedef dosyasi flush edilemedi",
            err,
        )
    })?;
    Ok(total)
}

fn query_total_entries(connection: &Connection) -> AmeleResult<u64> {
    connection
        .query_row("SELECT COUNT(*) FROM Files", [], |row| row.get::<_, i64>(0))
        .map(|value| value.max(0) as u64)
        .map_err(|err| {
            AmeleError::new(
                HataKodu::IcerikGecersiz,
                format!("Manifest kayit sayisi okunamadi: {err}"),
            )
        })
}

fn count_manifest_entries(manifest_db: &Path) -> AmeleResult<u64> {
    let connection = Connection::open_with_flags(
        manifest_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| AmeleError::new(HataKodu::IcerikGecersiz, err.to_string()))?;
    query_total_entries(&connection)
}

fn backup_file_path(backup_path: &Path, file_id: &str) -> Option<PathBuf> {
    if file_id.len() < 2 {
        return None;
    }
    Some(backup_path.join(&file_id[..2]).join(file_id))
}

fn map_ios_path(output_root: &Path, domain: &str, relative_path: &str) -> PathBuf {
    let base = domain_base_path(domain);
    let normalized_relative = relative_path.replace('\\', "/").replace(':', "/");
    let mut path = output_root.to_path_buf();

    for part in base.split('/') {
        push_safe_segment(&mut path, part);
    }
    for part in normalized_relative.split('/') {
        push_safe_segment(&mut path, part);
    }
    path
}

fn domain_base_path(domain: &str) -> String {
    const EXACT: &[(&str, &str)] = &[
        ("CameraRollDomain", "private/var/mobile"),
        ("DatabaseDomain", "private/var/db"),
        ("HealthDomain", "private/var/mobile/Library"),
        ("HomeDomain", "private/var/mobile"),
        ("HomeKitDomain", "private/var/mobile"),
        ("InstallDomain", "private/var/installd"),
        ("KeyboardDomain", "private/var/mobile"),
        ("KeychainDomain", "private/var/Keychains"),
        (
            "ManagedPreferencesDomain",
            "private/var/Managed Preferences",
        ),
        ("MediaDomain", "private/var/mobile/Media"),
        ("MobileDeviceDomain", "private/var/MobileDevice"),
        ("NetworkDomain", "private/var/networkd"),
        ("ProtectedDomain", "private/var/protected"),
        ("RootDomain", "private/var/root"),
        ("SystemPreferencesDomain", "private/var/preferences"),
        ("TonesDomain", "private/var/mobile"),
        ("WirelessDomain", "private/var/wireless"),
    ];
    const PREFIX: &[(&str, &str)] = &[
        (
            "AppDomain-",
            "private/var/mobile/Containers/Data/Application",
        ),
        (
            "AppDomainGroup-",
            "private/var/mobile/Containers/Shared/AppGroup",
        ),
        (
            "AppDomainPlugin-",
            "private/var/mobile/Containers/Data/PluginKitPlugin",
        ),
        ("SysContainerDomain-", "private/var/containers/Data/System"),
        (
            "SysSharedContainerDomain-",
            "private/var/containers/Shared/SystemGroup",
        ),
    ];

    if let Some((_, mapped)) = EXACT.iter().find(|(key, _)| *key == domain) {
        return (*mapped).to_string();
    }
    if let Some((prefix, mapped)) = PREFIX
        .iter()
        .find(|(prefix, _)| domain.starts_with(*prefix))
    {
        let suffix = sanitize_segment(&domain[prefix.len()..]);
        return format!("{mapped}/{suffix}");
    }

    let fallback = sanitize_segment(domain);
    if fallback.is_empty() {
        "private/var/Other".to_string()
    } else {
        format!("private/var/Other/{fallback}")
    }
}

fn push_safe_segment(path: &mut PathBuf, segment: &str) {
    let segment = sanitize_segment(segment);
    if !segment.is_empty() {
        path.push(segment);
    }
}

fn sanitize_segment(segment: &str) -> String {
    let mut out: String = segment
        .chars()
        .filter(|ch| {
            !ch.is_control() && !matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .collect();
    out = out
        .trim()
        .trim_matches('.')
        .trim_matches(char::from(0))
        .to_string();
    if out == "." || out == ".." {
        String::new()
    } else {
        out
    }
}

fn init_csv_log(path: &Path) -> AmeleResult<()> {
    let mut file = File::create(path).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!("iOS CSV log olusturulamadi: {}", path.display()),
            err,
        )
    })?;
    writeln!(file, "# Amele iOS Backup2FS normalization log")?;
    writeln!(
        file,
        "# Created: {}",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    )?;
    writeln!(
        file,
        "Timestamp,Status,Domain,RelativePath,FileID,OutputPath,SizeBytes,MD5,SHA1,SHA256"
    )?;
    Ok(())
}

fn append_csv_log(
    path: &Path,
    status: &str,
    entry: &BackupEntry,
    output_path: Option<&Path>,
    size: Option<u64>,
    hashes: Option<&[(HashAlgorithm, String)]>,
) -> AmeleResult<()> {
    let mut md5 = String::new();
    let mut sha1 = String::new();
    let mut sha256 = String::new();
    if let Some(hashes) = hashes {
        for (algorithm, value) in hashes {
            match algorithm {
                HashAlgorithm::Md5 => md5 = value.clone(),
                HashAlgorithm::Sha1 => sha1 = value.clone(),
                HashAlgorithm::Sha256 => sha256 = value.clone(),
                HashAlgorithm::Sha512 => {}
            }
        }
    }

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "iOS CSV log acilamadi", err))?;
    let out = output_path
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let line = [
        Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status.to_string(),
        entry.domain.clone(),
        entry.relative_path.clone(),
        format_file_id(&entry.file_id),
        out,
        size.map(|value| value.to_string()).unwrap_or_default(),
        md5,
        sha1,
        sha256,
    ]
    .into_iter()
    .map(|value| csv_field(&value))
    .collect::<Vec<_>>()
    .join(",");
    writeln!(file, "{line}")?;
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn format_file_id(file_id: &str) -> String {
    if file_id.len() >= 2 {
        format!("{}/{}", &file_id[..2], file_id)
    } else {
        file_id.to_string()
    }
}

fn write_ios_manifest(
    manifest_path: &Path,
    info: &IosBackupInfo,
    counters: &NormalizeCounters,
    log_path: &Path,
    output_dir: &Path,
) -> AmeleResult<()> {
    let manifest = json!({
        "tool": "Amele iOS Backup2FS",
        "created_at": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "source_backup": info.backup_path,
        "output_dir": output_dir,
        "device": {
            "name": info.device_name,
            "model": info.model,
            "product_type": info.product_type,
            "ios_version": info.ios_version,
            "build_version": info.build_version,
            "serial_number": info.serial_number,
            "unique_device_id": info.unique_device_id,
            "phone_number": info.phone_number,
            "last_backup_date": info.last_backup_date,
            "installed_apps_count": info.installed_apps_count,
        },
        "backup": {
            "encrypted": info.encrypted,
            "manifest_db": info.manifest_db,
            "manifest_plist": info.manifest_plist,
            "info_plist": info.info_plist,
            "status_plist": info.status_plist,
        },
        "summary": {
            "total_entries": counters.total_entries,
            "files_copied": counters.files_copied,
            "directories": counters.directories,
            "symlinks": counters.symlinks,
            "missing": counters.missing,
            "errors": counters.errors,
            "total_bytes": counters.total_bytes,
        },
        "log_path": log_path,
    });
    let content = serde_json::to_string_pretty(&manifest)?;
    fs::write(manifest_path, content).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!("iOS manifest yazilamadi: {}", manifest_path.display()),
            err,
        )
    })
}

fn read_plist_dictionary(path: &Path) -> Option<plist::Dictionary> {
    if !path.is_file() {
        return None;
    }
    let value = PlistValue::from_file(path).ok()?;
    value.into_dictionary()
}

fn plist_string(dict: Option<&plist::Dictionary>, key: &str) -> Option<String> {
    let value = dict?.get(key)?;
    value
        .as_string()
        .map(str::to_string)
        .or_else(|| value.as_unsigned_integer().map(|item| item.to_string()))
        .or_else(|| value.as_signed_integer().map(|item| item.to_string()))
        .or_else(|| value.as_boolean().map(|item| item.to_string()))
}

fn installed_apps_count(info: Option<&plist::Dictionary>) -> usize {
    let Some(info) = info else {
        return 0;
    };
    if let Some(apps) = info.get("Applications").and_then(PlistValue::as_dictionary) {
        return apps.len();
    }
    info.get("Installed Applications")
        .and_then(PlistValue::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn ios_model_name(product_type: &str) -> &str {
    match product_type {
        "iPhone15,2" => "iPhone 14 Pro",
        "iPhone15,3" => "iPhone 14 Pro Max",
        "iPhone15,4" => "iPhone 15",
        "iPhone15,5" => "iPhone 15 Plus",
        "iPhone16,1" => "iPhone 15 Pro",
        "iPhone16,2" => "iPhone 15 Pro Max",
        "iPhone17,1" => "iPhone 16 Pro",
        "iPhone17,2" => "iPhone 16 Pro Max",
        "iPhone17,3" => "iPhone 16",
        "iPhone17,4" => "iPhone 16 Plus",
        "iPhone17,5" => "iPhone 16e",
        _ => product_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_ios_domains() {
        let root = PathBuf::from("/case/ios");
        let path = map_ios_path(&root, "HomeDomain", "Library/SMS/sms.db");
        assert!(path.ends_with("private/var/mobile/Library/SMS/sms.db"));

        let app_path = map_ios_path(&root, "AppDomain-com.example.app", "Documents:data.db");
        assert!(app_path.ends_with(
            "private/var/mobile/Containers/Data/Application/com.example.app/Documents/data.db"
        ));
    }

    #[test]
    fn sanitizes_relative_paths() {
        let root = PathBuf::from("/case/ios");
        let path = map_ios_path(&root, "OtherDomain", "../bad|name:file?.txt");
        let rendered = path.display().to_string();
        assert!(!rendered.contains(".."));
        assert!(!rendered.contains('|'));
        assert!(!rendered.contains('?'));
    }
}
