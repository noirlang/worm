//! Kullanıcı ayarları ve varsayılan uygulama tercihlerini tanımlar.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 4444;
pub const DEFAULT_SIZE_MB: u64 = 100;
pub const DEFAULT_DETECTION_INTERVAL_MS: u64 = 3000;
pub const DEFAULT_CHUNK_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
/// Kullanıcının kalıcı uygulama ayarlarını temsil eder.
pub struct AppSettings {
    pub varsayilan_port: u16,
    pub varsayilan_boyut_mb: u64,
    pub disk_algilama_araligi_ms: u64,
    pub cikti_klasoru: PathBuf,
    pub vaka_klasoru: PathBuf,
    pub otomatik_rapor: bool,
    pub karanlik_tema: bool,
    pub dil: String,
    pub hash_algoritmasi: String,
    pub parca_boyutu: usize,
}

impl Default for AppSettings {
    /// Amele ana klasörü altında güvenli varsayılan ayarları üretir.
    fn default() -> Self {
        let amele_dir = crate::profile::active_username()
            .map(|username| crate::profile::profile_dir_for_username(&username))
            .unwrap_or_else(|| home_dir().join("Amele"));
        Self {
            varsayilan_port: DEFAULT_PORT,
            varsayilan_boyut_mb: DEFAULT_SIZE_MB,
            disk_algilama_araligi_ms: DEFAULT_DETECTION_INTERVAL_MS,
            cikti_klasoru: amele_dir.join("Ciktilar"),
            vaka_klasoru: amele_dir.join("Vakalar"),
            otomatik_rapor: true,
            karanlik_tema: false,
            dil: "tr".to_string(),
            hash_algoritmasi: "sha256".to_string(),
            parca_boyutu: DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Kalıcı kullanıcı ayarlarının varsayılan dosya yolunu döndürür.
pub fn default_settings_path() -> PathBuf {
    if let Some(path) = crate::profile::active_settings_path() {
        return path;
    }
    home_dir().join("Amele").join("ayarlar.json")
}

impl AppSettings {
    /// Ayar dosyasını okur; dosya yoksa varsayılan ayar döndürür.
    pub fn load(path: impl AsRef<Path>) -> AmeleResult<Self> {
        let path = path.as_ref();
        if !path.is_file() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .map_err(|err| AmeleError::io(HataKodu::DosyaOkuma, "Ayar dosyasi okunamadi", err))?;
        let mut settings: Self = serde_json::from_str(&content).map_err(|err| {
            AmeleError::new(
                HataKodu::ProtokolJson,
                format!("Ayar dosyasi parse edilemedi: {err}"),
            )
        })?;
        settings.normalize();
        Ok(settings)
    }

    /// Ayarları pretty JSON olarak diske yazar.
    pub fn save(&self, path: impl AsRef<Path>) -> AmeleResult<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "Ayar klasoru olusturulamadi", err)
            })?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Ayar dosyasi yazilamadi", err))
    }

    /// Eksik veya sıfır gelen ayarları güvenli varsayılanlara tamamlar.
    pub fn normalize(&mut self) {
        if self.varsayilan_port == 0 {
            self.varsayilan_port = DEFAULT_PORT;
        }
        if self.varsayilan_boyut_mb == 0 {
            self.varsayilan_boyut_mb = DEFAULT_SIZE_MB;
        }
        if self.disk_algilama_araligi_ms == 0 {
            self.disk_algilama_araligi_ms = DEFAULT_DETECTION_INTERVAL_MS;
        }
        if self.dil.is_empty() {
            self.dil = "tr".to_string();
        }
        if self.hash_algoritmasi.is_empty() {
            self.hash_algoritmasi = "sha256".to_string();
        }
        if self.parca_boyutu == 0 {
            self.parca_boyutu = DEFAULT_CHUNK_SIZE;
        }
    }
}

/// Platforma göre kullanıcının home klasörünü bulur.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                Some(PathBuf::from(format!(
                    "{}{}",
                    drive.to_string_lossy(),
                    path.to_string_lossy()
                )))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_c_application() {
        let settings = AppSettings::default();
        assert_eq!(settings.varsayilan_port, 4444);
        assert_eq!(settings.varsayilan_boyut_mb, 100);
        assert_eq!(settings.disk_algilama_araligi_ms, 3000);
        assert_eq!(settings.hash_algoritmasi, "sha256");
        assert_eq!(settings.parca_boyutu, 4 * 1024 * 1024);
    }
}
