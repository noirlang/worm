//! Vaka klasörü, kanıt kasası, notlar ve çıktı dizini yönetimini sağlar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::hash::{HashAlgorithm, calculate_file_hash};
use crate::logging::{LogLevel, Logger, runtime_log};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Vaka klasöründeki çıktı, mobil, hash ve rapor sayılarını özetler.
pub struct EvidenceSummary {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
    pub output_count: usize,
    pub android_count: usize,
    pub ios_count: usize,
    pub docker_count: usize,
    pub hash_count: usize,
    pub report_count: usize,
    pub manifest_path: PathBuf,
}

/// Bir vaka için tüm alt klasörleri, logger'ı ve dosya işlemlerini yöneten kasadır.
pub struct EvidenceVault {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub outputs_dir: PathBuf,
    pub ram_dir: PathBuf,
    pub android_dir: PathBuf,
    pub ios_dir: PathBuf,
    pub docker_dir: PathBuf,
    pub reports_dir: PathBuf,
    pub hash_dir: PathBuf,
    pub notes_dir: PathBuf,
    pub logger: Option<Logger>,
    lock: Mutex<()>,
}

impl EvidenceVault {
    /// Vaka klasör ağacını oluşturur ve günlük kaydını başlatır.
    pub fn create(base_dir: impl AsRef<Path>, case_name: impl AsRef<str>) -> AmeleResult<Self> {
        let case_name = case_name.as_ref().to_string();
        runtime_log(
            LogLevel::Info,
            "evidence",
            format!("Vaka kasasi olusturuluyor: {}", case_name),
        );
        let case_dir = base_dir.as_ref().join(&case_name);
        let logs_dir = case_dir.join("gunlukler");
        let outputs_dir = case_dir.join("ciktilar");
        let ram_dir = case_dir.join("ram");
        let android_dir = case_dir.join("android");
        let ios_dir = case_dir.join("ios");
        let docker_dir = case_dir.join("docker");
        let reports_dir = case_dir.join("raporlar");
        let hash_dir = case_dir.join("hash");
        let notes_dir = case_dir.join("notlar");

        for dir in [&case_dir, &logs_dir, &reports_dir, &hash_dir, &notes_dir] {
            runtime_log(
                LogLevel::Debug,
                "evidence",
                format!("Vaka temel alt dizini olusturuluyor: {}", dir.display()),
            );
            fs::create_dir_all(dir).map_err(|err| {
                let w_err = AmeleError::io(
                    HataKodu::DosyaYazma,
                    format!("Vaka dizini olusturulamadi: {}", dir.display()),
                    err,
                );
                runtime_log(
                    LogLevel::Error,
                    "evidence",
                    format!("Klasor olusturma hatasi: {:?}", w_err),
                );
                w_err
            })?;
        }

        let logger = Logger::start(&case_name, &logs_dir).ok();
        if let Some(logger) = &logger {
            logger.info(format!("Vaka olusturuldu: {case_name}"));
            logger.info(format!("Vaka klasoru: {}", case_dir.display()));
        }
        write_case_metadata(&case_name, &case_dir)?;
        runtime_log(
            LogLevel::Info,
            "evidence",
            format!("Vaka kasasi basariyla olusturuldu: {}", case_dir.display()),
        );

        let vault = Self {
            case_name,
            case_dir,
            logs_dir,
            outputs_dir,
            ram_dir,
            android_dir,
            ios_dir,
            docker_dir,
            reports_dir,
            hash_dir,
            notes_dir,
            logger,
            lock: Mutex::new(()),
        };
        let _ = vault.write_case_manifest();
        Ok(vault)
    }

    /// Belirli kasa alt klasöründe yeni çıktı dosyası yolu üretir ve klasörü talep anında oluşturur.
    pub fn new_file(&self, subdir: &str, file_name: &str) -> PathBuf {
        let _guard = self.lock.lock().ok();
        let target_dir = self.resolve_subdir(subdir);
        let _ = fs::create_dir_all(target_dir);
        target_dir.join(file_name)
    }

    /// Kasa alt klasörünün varlığını garanti eder; yoksa talep anında oluşturur.
    pub fn ensure_subdir(&self, subdir: &str) -> AmeleResult<PathBuf> {
        let _guard = self.lock.lock().ok();
        let target_dir = self.resolve_subdir(subdir).to_path_buf();
        fs::create_dir_all(&target_dir).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                format!("Vaka alt dizini olusturulamadi: {}", target_dir.display()),
                err,
            )
        })?;
        Ok(target_dir)
    }

    /// Kullanıcı notunu zaman damgalı dosya olarak notlar klasörüne yazar.
    pub fn add_note(&self, note: &str) -> AmeleResult<PathBuf> {
        let _guard = self.lock.lock().ok();
        let now = Local::now();
        let file_name = format!("not_{}.txt", now.format("%Y%m%d_%H%M%S"));
        let path = self.notes_dir.join(&file_name);
        runtime_log(
            LogLevel::Info,
            "evidence",
            format!("Vaka notu yaziliyor: {}", path.display()),
        );
        let content = format!(
            "Vaka: {}\nTarih: {}\n========================================\n\n{}\n",
            self.case_name,
            now.format("%Y-%m-%d %H:%M:%S"),
            note
        );
        fs::write(&path, content).map_err(|err| {
            let w_err = AmeleError::io(HataKodu::DosyaYazma, "Not yazilamadi", err);
            runtime_log(
                LogLevel::Error,
                "evidence",
                format!("Not yazma hatasi: {:?}", w_err),
            );
            w_err
        })?;
        if let Some(logger) = &self.logger {
            logger.info(format!("Not eklendi: {file_name}"));
        }
        runtime_log(
            LogLevel::Info,
            "evidence",
            format!("Vaka notu basariyla eklendi: {}", file_name),
        );
        drop(_guard);
        let _ = self.write_case_manifest();
        Ok(path)
    }

    /// Kasa alt klasöründeki dosyaları listeler.
    pub fn list_files(&self, subdir: &str) -> AmeleResult<Vec<PathBuf>> {
        let dir = self.resolve_subdir(subdir);
        runtime_log(
            LogLevel::Debug,
            "evidence",
            format!("Vaka dizini taranıyor ({}): {}", subdir, dir.display()),
        );
        let mut files = Vec::new();
        if !dir.is_dir() {
            runtime_log(
                LogLevel::Debug,
                "evidence",
                format!(
                    "Klasor mevcut degil, bos liste donuluyor: {}",
                    dir.display()
                ),
            );
            return Ok(files);
        }

        for entry in fs::read_dir(dir).map_err(|err| {
            let w_err = AmeleError::io(HataKodu::DosyaOkuma, "Dizin okunamadi", err);
            runtime_log(
                LogLevel::Error,
                "evidence",
                format!("Dizin okuma hatasi: {:?}", w_err),
            );
            w_err
        })? {
            let entry = entry.map_err(|err| {
                let w_err = AmeleError::io(HataKodu::DosyaOkuma, "Dizin girdisi okunamadi", err);
                runtime_log(
                    LogLevel::Error,
                    "evidence",
                    format!("Dizin girdisi okuma hatasi: {:?}", w_err),
                );
                w_err
            })?;
            files.push(entry.path());
        }
        Ok(files)
    }

    /// Vaka kasasının güncel dosya sayılarını döndürür.
    pub fn summary(&self) -> AmeleResult<EvidenceSummary> {
        let metadata = read_case_metadata(&self.case_dir);
        Ok(EvidenceSummary {
            case_name: self.case_name.clone(),
            case_dir: self.case_dir.clone(),
            created_by: metadata.created_by,
            created_by_name: metadata.created_by_name,
            output_count: self.list_files("ciktilar")?.len(),
            android_count: self.list_files("android")?.len(),
            ios_count: self.list_files("ios")?.len(),
            docker_count: self.list_files("docker")?.len(),
            hash_count: self.list_files("hash")?.len(),
            report_count: self.list_files("raporlar")?.len(),
            manifest_path: self.case_manifest_path(),
        })
    }

    /// Vaka içindeki dosya envanterini ve SHA-256 bütünlük özetini yazar.
    pub fn write_case_manifest(&self) -> AmeleResult<PathBuf> {
        let manifest_path = self.case_manifest_path();
        let files = collect_manifest_files(&self.case_dir, &manifest_path);
        let manifest = json!({
            "tool": "Amele Forensic Tool",
            "tool_version": env!("CARGO_PKG_VERSION"),
            "manifest_version": 1,
            "generated_at": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "case": {
                "name": &self.case_name,
                "dir": &self.case_dir,
                "metadata": read_case_metadata(&self.case_dir),
            },
            "folders": {
                "logs": &self.logs_dir,
                "outputs": &self.outputs_dir,
                "ram": &self.ram_dir,
                "android": &self.android_dir,
                "ios": &self.ios_dir,
                "docker": &self.docker_dir,
                "reports": &self.reports_dir,
                "hash": &self.hash_dir,
                "notes": &self.notes_dir,
            },
            "counts": {
                "outputs": count_entries_recursive(&self.outputs_dir),
                "ram": count_entries_recursive(&self.ram_dir),
                "android": count_entries_recursive(&self.android_dir),
                "ios": count_entries_recursive(&self.ios_dir),
                "docker": count_entries_recursive(&self.docker_dir),
                "reports": count_entries_recursive(&self.reports_dir),
                "hash": count_entries_recursive(&self.hash_dir),
                "notes": count_entries_recursive(&self.notes_dir),
                "logs": count_entries_recursive(&self.logs_dir),
                "files": files.len(),
            },
            "files": files,
        });
        let content = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, content).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                format!("Vaka manifesti yazilamadi: {}", manifest_path.display()),
                err,
            )
        })?;
        Ok(manifest_path)
    }

    /// Vaka bütünlük manifestinin standart yolunu döndürür.
    pub fn case_manifest_path(&self) -> PathBuf {
        self.case_dir.join("case_manifest.json")
    }

    /// Kullanıcı/API alt klasör adını gerçek kasa klasörüne eşler.
    fn resolve_subdir(&self, subdir: &str) -> &Path {
        match subdir {
            "gunlukler" => &self.logs_dir,
            "ciktilar" => &self.outputs_dir,
            "ram" => &self.ram_dir,
            "android" => &self.android_dir,
            "ios" => &self.ios_dir,
            "docker" => &self.docker_dir,
            "raporlar" => &self.reports_dir,
            "hash" => &self.hash_dir,
            "notlar" => &self.notes_dir,
            _ => &self.case_dir,
        }
    }
}

fn collect_manifest_files(case_dir: &Path, manifest_path: &Path) -> Vec<serde_json::Value> {
    let mut files = Vec::new();
    collect_manifest_files_recursive(case_dir, case_dir, manifest_path, &mut files);
    files.sort_by(|left, right| {
        left["relative_path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["relative_path"].as_str().unwrap_or_default())
    });
    files
}

fn collect_manifest_files_recursive(
    root: &Path,
    dir: &Path,
    manifest_path: &Path,
    files: &mut Vec<serde_json::Value>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == manifest_path {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            collect_manifest_files_recursive(root, &path, manifest_path, files);
            continue;
        }
        files.push(case_manifest_file_json(root, &path, &metadata));
    }
}

fn case_manifest_file_json(root: &Path, path: &Path, metadata: &fs::Metadata) -> serde_json::Value {
    let relative = relative_case_path(root, path);
    let file_type = if metadata.file_type().is_symlink() {
        "symlink"
    } else {
        "file"
    };
    let sha256 = if metadata.file_type().is_file() {
        calculate_file_hash(path, HashAlgorithm::Sha256).ok()
    } else {
        None
    };
    let symlink_target = if metadata.file_type().is_symlink() {
        fs::read_link(path)
            .ok()
            .map(|target| target.to_string_lossy().to_string())
    } else {
        None
    };

    json!({
        "relative_path": relative,
        "path": path,
        "folder": relative.split('/').next().unwrap_or_default(),
        "type": file_type,
        "size_bytes": metadata.len(),
        "modified_at": metadata.modified().ok().map(format_system_time),
        "sha256": sha256,
        "symlink_target": symlink_target,
    })
}

pub fn relative_case_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn format_system_time(time: std::time::SystemTime) -> String {
    chrono::DateTime::<Local>::from(time)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn count_entries_recursive(path: &Path) -> usize {
    let mut count = 0;
    count_entries_recursive_inner(path, &mut count);
    count
}

fn count_entries_recursive_inner(path: &Path, count: &mut usize) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        *count += 1;
        if path.is_dir() {
            count_entries_recursive_inner(&path, count);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
/// Vaka sahibini ve ilk oluşturma bilgisini saklayan metadata dosyasıdır.
pub struct EvidenceMetadata {
    pub case_name: String,
    pub created_at: String,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
}

/// Vaka metadata dosyasını oluşturur; mevcut dosyayı ezmez.
fn write_case_metadata(case_name: &str, case_dir: &Path) -> AmeleResult<()> {
    let path = case_dir.join("vaka.json");
    if path.is_file() {
        return Ok(());
    }
    let profile = crate::profile::active_profile();
    let metadata = EvidenceMetadata {
        case_name: case_name.to_string(),
        created_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        created_by: profile.as_ref().map(|profile| profile.username.clone()),
        created_by_name: profile.as_ref().map(|profile| profile.full_name.clone()),
    };
    let content = serde_json::to_string_pretty(&metadata)?;
    fs::write(&path, content).map_err(|err| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!("Vaka metadata yazılamadı: {}", path.display()),
            err,
        )
    })
}

/// Vaka metadata dosyasını okur.
pub fn read_case_metadata(case_dir: &Path) -> EvidenceMetadata {
    let path = case_dir.join("vaka.json");
    let Ok(content) = fs::read_to_string(path) else {
        return EvidenceMetadata::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_case_tree_and_notes() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "case1").unwrap();
        assert!(vault.logs_dir.is_dir());
        assert!(vault.notes_dir.is_dir());
        // Edinim yapılan klasörler talep anında (on-demand) oluşturulur
        assert!(!vault.android_dir.is_dir());
        assert!(!vault.ios_dir.is_dir());
        assert!(!vault.docker_dir.is_dir());
        assert!(!vault.outputs_dir.is_dir());
        let _ = vault.ensure_subdir("android");
        assert!(vault.android_dir.is_dir());
        let note = vault.add_note("hello").unwrap();
        assert!(note.is_file());
        let summary = vault.summary().unwrap();
        assert_eq!(summary.case_name, "case1");
    }

    #[test]
    fn writes_case_integrity_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "case_manifest").unwrap();
        fs::write(vault.new_file("ciktilar", "sample.txt"), "sample evidence").unwrap();

        let manifest_path = vault.write_case_manifest().unwrap();
        assert!(manifest_path.is_file());

        let content = fs::read_to_string(&manifest_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();
        let files = manifest["files"].as_array().unwrap();
        let sample = files
            .iter()
            .find(|entry| entry["relative_path"] == "ciktilar/sample.txt")
            .unwrap();

        assert_eq!(sample["type"], "file");
        assert_eq!(sample["sha256"].as_str().unwrap().len(), 64);
        assert!(
            files
                .iter()
                .all(|entry| entry["relative_path"] != "case_manifest.json")
        );
    }
}
