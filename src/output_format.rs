//! Edinim çıktısını RAW veya AFF4 paket biçimine dönüştürür.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::hash::{self, HashAlgorithm};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Desteklenen edinim çıktı formatlarıdır.
pub enum AcquisitionOutputFormat {
    Raw,
    Aff4,
}

#[derive(Debug, Clone)]
/// Edinim sırasında yazılacak geçici/final hedef yollarını taşır.
pub struct OutputPlan {
    pub format: AcquisitionOutputFormat,
    pub working_path: PathBuf,
    pub final_path: PathBuf,
}

#[derive(Debug, Clone)]
/// Final format dönüşümünden sonra API/CLI'ye dönecek bilgidir.
pub struct FinalizedOutput {
    pub target_path: PathBuf,
    pub sha256: String,
    pub raw_sha256: Option<String>,
    pub format: AcquisitionOutputFormat,
}

impl AcquisitionOutputFormat {
    /// Kullanıcı/API değerini çıktı formatına çevirir.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("raw")
            .to_ascii_lowercase()
            .as_str()
        {
            "raw" | "dd" | "img" => Ok(Self::Raw),
            "aff4" => Ok(Self::Aff4),
            other => Err(format!("output_format raw veya aff4 olmalıdır: {other}")),
        }
    }

    /// JSON ve UI için kısa format adını döndürür.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Aff4 => "aff4",
        }
    }
}

/// İstenen final format için çalışma ve final yollarını üretir.
pub fn plan_output(target: impl AsRef<Path>, format: AcquisitionOutputFormat) -> OutputPlan {
    let target = target.as_ref();
    match format {
        AcquisitionOutputFormat::Raw => OutputPlan {
            format,
            working_path: target.to_path_buf(),
            final_path: target.to_path_buf(),
        },
        AcquisitionOutputFormat::Aff4 => {
            let final_path = target.with_extension("aff4");
            let working_path = final_path.with_extension("aff4.raw");
            OutputPlan {
                format,
                working_path,
                final_path,
            }
        }
    }
}

/// Edinim çıktısını seçilen final formata tamamlar.
pub fn finalize_output(
    plan: &OutputPlan,
    artifact_kind: &str,
    source_label: &str,
    case_name: &str,
    existing_raw_sha256: Option<String>,
) -> Result<FinalizedOutput, String> {
    match plan.format {
        AcquisitionOutputFormat::Raw => {
            let sha256 = match existing_raw_sha256.filter(|value| !value.trim().is_empty()) {
                Some(value) => value,
                None => hash::calculate_file_hash(&plan.working_path, HashAlgorithm::Sha256)
                    .map_err(|err| err.to_string())?,
            };
            hash::write_sha256_sidecar(&plan.working_path, &sha256)
                .map_err(|err| err.to_string())?;
            Ok(FinalizedOutput {
                target_path: plan.working_path.clone(),
                sha256,
                raw_sha256: None,
                format: plan.format,
            })
        }
        AcquisitionOutputFormat::Aff4 => {
            let raw_sha256 = match existing_raw_sha256.filter(|value| !value.trim().is_empty()) {
                Some(value) => value,
                None => hash::calculate_file_hash(&plan.working_path, HashAlgorithm::Sha256)
                    .map_err(|err| err.to_string())?,
            };
            package_aff4(plan, artifact_kind, source_label, case_name, &raw_sha256)
                .map_err(|err| err.to_string())?;
            let aff4_sha256 = hash::calculate_file_hash(&plan.final_path, HashAlgorithm::Sha256)
                .map_err(|err| err.to_string())?;
            hash::write_sha256_sidecar(&plan.final_path, &aff4_sha256)
                .map_err(|err| err.to_string())?;
            let _ = fs::remove_file(&plan.working_path);
            let _ = fs::remove_file(plan.working_path.with_extension("aff4.raw.sha256"));
            Ok(FinalizedOutput {
                target_path: plan.final_path.clone(),
                sha256: aff4_sha256,
                raw_sha256: Some(raw_sha256),
                format: plan.format,
            })
        }
    }
}

/// Basit AFF4 kanıt paketini manifest ve veri girdileriyle oluşturur.
fn package_aff4(
    plan: &OutputPlan,
    artifact_kind: &str,
    source_label: &str,
    case_name: &str,
    raw_sha256: &str,
) -> AmeleResult<()> {
    if let Some(parent) = plan.final_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                "AFF4 hedef klasörü oluşturulamadı",
                err,
            )
        })?;
    }

    let raw_file = File::open(&plan.working_path)
        .map_err(|err| AmeleError::io(HataKodu::DosyaOkuma, "RAW veri açılamadı", err))?;
    let raw_size = raw_file
        .metadata()
        .map_err(|err| AmeleError::io(HataKodu::DosyaOkuma, "RAW metadata okunamadı", err))?
        .len();
    let aff4_file = File::create(&plan.final_path)
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "AFF4 paketi oluşturulamadı", err))?;
    let mut builder = tar::Builder::new(aff4_file);
    let data_name = plan
        .working_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("evidence.raw")
        .to_string();

    let manifest = json!({
        "format": "aff4",
        "container": "tar-aff4-evidence-package",
        "created_at": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "artifact_kind": artifact_kind,
        "source": source_label,
        "case_name": case_name,
        "operator": crate::profile::active_profile(),
        "data_file": data_name,
        "raw_size": raw_size,
        "raw_sha256": raw_sha256,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let mut manifest_header = tar::Header::new_gnu();
    manifest_header.set_size(manifest_bytes.len() as u64);
    manifest_header.set_mode(0o644);
    manifest_header.set_cksum();
    builder
        .append_data(
            &mut manifest_header,
            "manifest.json",
            manifest_bytes.as_slice(),
        )
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "AFF4 manifest yazılamadı", err))?;

    let mut data_header = tar::Header::new_gnu();
    data_header.set_size(raw_size);
    data_header.set_mode(0o644);
    data_header.set_cksum();
    builder
        .append_data(&mut data_header, data_name, raw_file)
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "AFF4 veri yazılamadı", err))?;
    builder
        .finish()
        .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "AFF4 paket kapatılamadı", err))
}
