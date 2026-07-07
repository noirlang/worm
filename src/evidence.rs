//! Vaka klasörü, kanıt kasası, notlar ve çıktı dizini yönetimini sağlar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::logging::{LogLevel, Logger, runtime_log};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Vaka klasöründeki çıktı, Android, hash ve rapor sayılarını özetler.
pub struct EvidenceSummary {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub created_by: Option<String>,
    pub created_by_name: Option<String>,
    pub output_count: usize,
    pub android_count: usize,
    pub hash_count: usize,
    pub report_count: usize,
}

/// Bir vaka için tüm alt klasörleri, logger'ı ve dosya işlemlerini yöneten kasadır.
pub struct EvidenceVault {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub outputs_dir: PathBuf,
    pub ram_dir: PathBuf,
    pub android_dir: PathBuf,
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
        let reports_dir = case_dir.join("raporlar");
        let hash_dir = case_dir.join("hash");
        let notes_dir = case_dir.join("notlar");

        for dir in [
            &case_dir,
            &logs_dir,
            &outputs_dir,
            &ram_dir,
            &android_dir,
            &reports_dir,
            &hash_dir,
            &notes_dir,
        ] {
            runtime_log(
                LogLevel::Debug,
                "evidence",
                format!("Vaka alt dizini olusturuluyor: {}", dir.display()),
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

        Ok(Self {
            case_name,
            case_dir,
            logs_dir,
            outputs_dir,
            ram_dir,
            android_dir,
            reports_dir,
            hash_dir,
            notes_dir,
            logger,
            lock: Mutex::new(()),
        })
    }

    /// Belirli kasa alt klasöründe yeni çıktı dosyası yolu üretir.
    pub fn new_file(&self, subdir: &str, file_name: &str) -> PathBuf {
        let _guard = self.lock.lock().ok();
        self.resolve_subdir(subdir).join(file_name)
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
            hash_count: self.list_files("hash")?.len(),
            report_count: self.list_files("raporlar")?.len(),
        })
    }

    /// Kullanıcı/API alt klasör adını gerçek kasa klasörüne eşler.
    fn resolve_subdir(&self, subdir: &str) -> &Path {
        match subdir {
            "gunlukler" => &self.logs_dir,
            "ciktilar" => &self.outputs_dir,
            "ram" => &self.ram_dir,
            "android" => &self.android_dir,
            "raporlar" => &self.reports_dir,
            "hash" => &self.hash_dir,
            "notlar" => &self.notes_dir,
            _ => &self.case_dir,
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
        assert!(vault.outputs_dir.is_dir());
        assert!(vault.ram_dir.is_dir());
        assert!(vault.android_dir.is_dir());
        let note = vault.add_note("hello").unwrap();
        assert!(note.is_file());
        let summary = vault.summary().unwrap();
        assert_eq!(summary.case_name, "case1");
    }
}
