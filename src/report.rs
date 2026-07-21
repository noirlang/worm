//! Vaka ve edinim çıktılarından TXT/JSON rapor dosyaları üretir.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::evidence::EvidenceVault;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const REPORT_FILE_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Rapor oluşturma ekranından gelen temel başlık, kaynak ve hash bilgilerini taşır.
pub struct ReportInfo {
    pub title: String,
    pub description: String,
    pub creator: String,
    pub source: String,
    pub hash_sha256: String,
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Desteklenen rapor çıktı formatlarını belirtir.
pub enum ReportFormat {
    Txt,
    Json,
}

impl ReportFormat {
    /// Rapor formatına karşılık gelen dosya uzantısını döndürür.
    pub fn extension(self) -> &'static str {
        match self {
            ReportFormat::Txt => "txt",
            ReportFormat::Json => "json",
        }
    }
}

/// Vaka adı ve tarih içeren standart rapor dosya adı üretir.
pub fn new_report_file_name(case_name: &str, format: ReportFormat) -> String {
    format!(
        "rapor_{}_{}.{}",
        case_name,
        Local::now().format("%Y%m%d_%H%M%S"),
        format.extension()
    )
}

/// Tek dosya için ad, boyut ve değiştirme zamanı özeti üretir.
pub fn file_summary(path: impl AsRef<Path>) -> AmeleResult<String> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)
        .map_err(|err| AmeleError::io(HataKodu::DosyaAcilamadi, "Dosya bulunamadi", err))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let modified = metadata
        .modified()
        .ok()
        .map(chrono::DateTime::<Local>::from)
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "bilinmiyor".to_string());
    let (size_value, size_unit) = human_size(metadata.len());

    Ok(format!(
        "Dosya: {name}\nBoyut: {size_value:.2} {size_unit} ({} bayt)\nDegistirme: {modified}\n",
        metadata.len()
    ))
}

/// Var olan TXT raporun sonuna sistem bilgisini ekler.
pub fn append_system_info(path: impl AsRef<Path>) -> AmeleResult<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path.as_ref())
        .map_err(|err| AmeleError::io(HataKodu::DosyaAcilamadi, "Rapor dosyasi acilamadi", err))?;
    let info = system_info();
    writeln!(file, "\n========================================")
        .and_then(|_| writeln!(file, "SISTEM BILGISI"))
        .and_then(|_| writeln!(file, "========================================"))
        .and_then(|_| writeln!(file, "Isletim Sistemi: {}", info.os))
        .and_then(|_| writeln!(file, "Surum: {}", info.version))
        .and_then(|_| writeln!(file, "Makine: {}", info.machine))
        .and_then(|_| writeln!(file, "Hostname: {}", info.hostname))
        .and_then(|_| writeln!(file, "\nKullanici: {}", info.user))
        .and_then(|_| writeln!(file, "PID: {}", std::process::id()))
        .and_then(|_| {
            writeln!(
                file,
                "Rapor Tarihi: {}",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            )
        })
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Sistem bilgisi yazilamadi", err))
}

/// TXT veya JSON raporu hedef dosyaya oluşturur.
pub fn create_report(
    info: &ReportInfo,
    format: ReportFormat,
    target: impl AsRef<Path>,
    vault: Option<&EvidenceVault>,
) -> AmeleResult<PathBuf> {
    let target = target.as_ref();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Rapor klasoru olusturulamadi", err)
        })?;
    }

    match format {
        ReportFormat::Txt => {
            fs::write(target, render_txt(info, vault))
                .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Rapor yazilamadi", err))?;
            append_system_info(target)?;
        }
        ReportFormat::Json => {
            let system = system_info();
            let content = json!({
                "tur": "adli_bilisim_raporu",
                "versiyon": "1.0",
                "baslik": info.title,
                "aciklama": info.description,
                "olusturan": info.creator,
                "kaynak": info.source,
                "tarih": info.date,
                "hash_sha256": info.hash_sha256,
                "sistem": system,
                "vaka": vault.map(vault_report_json),
            });
            fs::write(target, serde_json::to_string_pretty(&content)?).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "JSON rapor yazilamadi", err)
            })?;
        }
    }

    if let Some(vault) = vault
        && let Some(logger) = &vault.logger
    {
        logger.info(format!("Rapor olusturuldu: {}", target.display()));
    }

    Ok(target.to_path_buf())
}

/// Rapor bilgilerini ve varsa vaka kasası özetini TXT metnine çevirir.
fn render_txt(info: &ReportInfo, vault: Option<&EvidenceVault>) -> String {
    let mut out = String::new();
    out.push_str("========================================\n");
    out.push_str("    ADLI BILISIM TEKNIK RAPORU\n");
    out.push_str("========================================\n\n");
    out.push_str(&format!("BASLIK: {}\n", info.title));
    out.push_str(&format!("ACIKLAMA: {}\n", info.description));
    out.push_str(&format!("OLUSTURAN: {}\n", info.creator));
    out.push_str(&format!("KAYNAK: {}\n", info.source));
    out.push_str(&format!("TARIH: {}\n", info.date));
    if !info.hash_sha256.is_empty() {
        out.push_str("\n----------------------------------------\n");
        out.push_str("HASH DEGERI (SHA-256):\n");
        out.push_str(&info.hash_sha256);
        out.push_str("\n----------------------------------------\n");
    }
    if let Some(vault) = vault {
        append_vault_summary_txt(&mut out, vault);
    }
    out.push_str("\n========================================\n");
    out.push_str("Sistem tarafindan olusturulmustur.\n");
    out.push_str("========================================\n");
    out
}

/// TXT rapora vaka kasası dosya özetini ekler.
fn append_vault_summary_txt(out: &mut String, vault: &EvidenceVault) {
    let android_count = count_files_recursive(&vault.android_dir);
    let ios_count = count_files_recursive(&vault.ios_dir);
    out.push_str("\n----------------------------------------\n");
    out.push_str("VAKA KASASI\n");
    out.push_str(&format!("Vaka: {}\n", vault.case_name));
    out.push_str(&format!("Klasor: {}\n", vault.case_dir.display()));
    out.push_str(&format!(
        "Ciktilar: {} | RAM: {} | Android: {} | iOS: {} | Hash: {} | Rapor: {}\n",
        count_directory_entries(&vault.outputs_dir),
        count_directory_entries(&vault.ram_dir),
        android_count,
        ios_count,
        count_directory_entries(&vault.hash_dir),
        count_directory_entries(&vault.reports_dir)
    ));
    out.push_str("\nANDROID CIKTILARI\n");
    out.push_str(&format!("Klasor: {}\n", vault.android_dir.display()));
    if android_count == 0 {
        out.push_str("Kayitli Android ciktisi yok.\n");
    } else {
        for entry in collect_file_entries(&vault.android_dir, 10) {
            let name = entry["name"].as_str().unwrap_or_default();
            let size = entry["size"].as_u64().unwrap_or_default();
            out.push_str(&format!("- {name} ({size} bayt)\n"));
        }
        if android_count > 10 {
            out.push_str(&format!("... {} dosya daha\n", android_count - 10));
        }
    }
    out.push_str("\niOS CIKTILARI\n");
    out.push_str(&format!("Klasor: {}\n", vault.ios_dir.display()));
    if ios_count == 0 {
        out.push_str("Kayitli iOS ciktisi yok.\n");
    } else {
        for entry in collect_file_entries(&vault.ios_dir, 10) {
            let name = entry["name"].as_str().unwrap_or_default();
            let size = entry["size"].as_u64().unwrap_or_default();
            out.push_str(&format!("- {name} ({size} bayt)\n"));
        }
        if ios_count > 10 {
            out.push_str(&format!("... {} dosya daha\n", ios_count - 10));
        }
    }
}

/// Vaka kasasını JSON rapor içine eklenecek yapıya dönüştürür.
fn vault_report_json(vault: &EvidenceVault) -> Value {
    json!({
        "case_name": &vault.case_name,
        "case_dir": &vault.case_dir,
        "folders": {
            "outputs": &vault.outputs_dir,
            "ram": &vault.ram_dir,
            "android": &vault.android_dir,
            "ios": &vault.ios_dir,
            "hash": &vault.hash_dir,
            "reports": &vault.reports_dir,
            "notes": &vault.notes_dir,
            "logs": &vault.logs_dir,
        },
        "counts": {
            "outputs": count_directory_entries(&vault.outputs_dir),
            "ram": count_directory_entries(&vault.ram_dir),
            "android": count_files_recursive(&vault.android_dir),
            "ios": count_files_recursive(&vault.ios_dir),
            "hash": count_directory_entries(&vault.hash_dir),
            "reports": count_directory_entries(&vault.reports_dir),
        },
        "android": {
            "dir": &vault.android_dir,
            "file_count": count_files_recursive(&vault.android_dir),
            "files": collect_file_entries(&vault.android_dir, REPORT_FILE_LIMIT),
        },
        "ios": {
            "dir": &vault.ios_dir,
            "file_count": count_files_recursive(&vault.ios_dir),
            "files": collect_file_entries(&vault.ios_dir, REPORT_FILE_LIMIT),
        },
    })
}

/// Bir klasördeki dosya girişlerini rapor limitiyle toplar.
fn collect_file_entries(dir: &Path, limit: usize) -> Vec<Value> {
    let mut files = Vec::new();
    collect_paths_recursive(dir, &mut files);
    files.sort();
    files
        .into_iter()
        .take(limit)
        .map(|path| file_entry_json(dir, path))
        .collect()
}

/// Klasörü rekürsif dolaşıp dosya yollarını biriktirir.
fn collect_paths_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_paths_recursive(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// Dosya yolunu raporda gösterilecek JSON girdisine dönüştürür.
fn file_entry_json(base: &Path, path: PathBuf) -> Value {
    let metadata = fs::metadata(&path).ok();
    let relative = path.strip_prefix(base).unwrap_or(&path);
    json!({
        "name": relative.to_string_lossy(),
        "path": path,
        "size": metadata.map(|meta| meta.len()).unwrap_or_default(),
    })
}

/// Klasördeki doğrudan girdi sayısını hesaplar.
fn count_directory_entries(path: &Path) -> usize {
    fs::read_dir(path)
        .map(|entries| entries.flatten().count())
        .unwrap_or_default()
}

/// Klasördeki tüm dosyaları rekürsif sayar.
fn count_files_recursive(path: &Path) -> usize {
    let mut files = Vec::new();
    collect_paths_recursive(path, &mut files);
    files.len()
}

/// Bayt değerini okunabilir sayı ve birime dönüştürür.
fn human_size(bytes: u64) -> (f64, &'static str) {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        (bytes as f64 / GB as f64, "GB")
    } else if bytes >= MB {
        (bytes as f64 / MB as f64, "MB")
    } else if bytes >= KB {
        (bytes as f64 / KB as f64, "KB")
    } else {
        (bytes as f64, "B")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemInfo {
    os: String,
    version: String,
    machine: String,
    hostname: String,
    user: String,
}

/// Çalışan sistemden rapora yazılacak platform bilgisini toplar.
fn system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        version: os_version(),
        machine: std::env::consts::ARCH.to_string(),
        hostname: hostname(),
        user: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "bilinmiyor".to_string()),
    }
}

#[cfg(unix)]
/// İşletim sistemi sürümünü platforma göre okur.
fn os_version() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "bilinmiyor".to_string())
}

#[cfg(windows)]
/// İşletim sistemi sürümünü platforma göre okur.
fn os_version() -> String {
    "Windows".to_string()
}

/// Makine hostname değerini platforma göre okur.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "bilinmiyor".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_report_stays_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("report.json");
        let info = ReportInfo {
            title: "T".to_string(),
            description: "D".to_string(),
            creator: "C".to_string(),
            source: "S".to_string(),
            hash_sha256: "abc".to_string(),
            date: "2026-05-15 00:00:00".to_string(),
        };
        create_report(&info, ReportFormat::Json, &target, None).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed["tur"], "adli_bilisim_raporu");
    }

    #[test]
    fn json_report_includes_android_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "case1").unwrap();
        std::fs::write(vault.android_dir.join("device_profile.json"), "{}").unwrap();
        let target = vault.reports_dir.join("report.json");
        let info = ReportInfo {
            title: "T".to_string(),
            description: "D".to_string(),
            creator: "C".to_string(),
            source: "S".to_string(),
            hash_sha256: "abc".to_string(),
            date: "2026-05-15 00:00:00".to_string(),
        };
        create_report(&info, ReportFormat::Json, &target, Some(&vault)).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed["vaka"]["counts"]["android"], 1);
        assert_eq!(
            parsed["vaka"]["android"]["files"][0]["name"],
            "device_profile.json"
        );
    }
}
