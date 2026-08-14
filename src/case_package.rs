//! Vaka paketleme ve açma işlemlerini (.amelecase arşiv formatı) yönetir.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::evidence::EvidenceVault;
use crate::hash::{HashAlgorithm, calculate_file_hash, write_sha256_sidecar};
use crate::logging::{LogLevel, runtime_log};
use chrono::Local;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasePackageHeader {
    pub format_version: u8,
    pub case_name: String,
    pub created_at: String,
    pub created_by: Option<String>,
    pub tool_version: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasePackageManifestEntry {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub files_extracted: usize,
    pub integrity_verified: bool,
    pub warnings: Vec<String>,
}

pub fn export_case(vault: &EvidenceVault, output_path: &Path) -> AmeleResult<PathBuf> {
    vault.write_case_manifest()?;

    let final_output_path = if output_path.is_dir() {
        output_path.join(format!("{}.amelecase", vault.case_name))
    } else {
        output_path.to_path_buf()
    };

    let tar_gz = File::create(&final_output_path)
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Paket dosyası oluşturulamadı", e))?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut builder = Builder::new(enc);

    fn count_files(dir: &Path) -> (usize, u64) {
        let mut count = 0;
        let mut size = 0;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        size += meta.len();
                    } else if meta.is_dir() {
                        let (c, s) = count_files(&entry.path());
                        count += c;
                        size += s;
                    }
                }
            }
        }
        (count, size)
    }

    let (file_count, total_bytes) = count_files(&vault.case_dir);
    let summary = vault.summary()?;

    let header = CasePackageHeader {
        format_version: 1,
        case_name: vault.case_name.clone(),
        created_at: Local::now().to_rfc3339(),
        created_by: summary.created_by,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        file_count,
        total_bytes,
    };

    let header_json = serde_json::to_string_pretty(&header)?;
    let mut header_header = tar::Header::new_gnu();
    header_header.set_size(header_json.len() as u64);
    header_header.set_mode(0o644);
    header_header.set_cksum();
    builder
        .append_data(
            &mut header_header,
            "amele_case_header.json",
            header_json.as_bytes(),
        )
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Header arşive eklenemedi", e))?;

    builder
        .append_dir_all(&vault.case_name, &vault.case_dir)
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Vaka dosyaları arşive eklenemedi", e))?;

    let enc = builder
        .into_inner()
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Tar arşivi kapatılamadı", e))?;
    enc.finish()
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Gz arşivi kapatılamadı", e))?;

    let hash = calculate_file_hash(&final_output_path, HashAlgorithm::Sha256)?;
    write_sha256_sidecar(&final_output_path, &hash)?;

    runtime_log(
        LogLevel::Info,
        "case_package",
        format!("Vaka dışa aktarıldı: {}", final_output_path.display()),
    );

    Ok(final_output_path)
}

pub fn import_case(package_path: &Path, target_base_dir: &Path) -> AmeleResult<ImportResult> {
    let tar_gz_header = File::open(package_path)
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Paket dosyası açılamadı", e))?;
    let tar_header = GzDecoder::new(tar_gz_header);
    let mut archive_header = Archive::new(tar_header);

    let mut header_data = None;
    if let Ok(entries) = archive_header.entries() {
        for file in entries.flatten() {
            if let Ok(path) = file.path() {
                if path.to_str().unwrap_or_default() == "amele_case_header.json" {
                    let mut content = String::new();
                    let mut f = file;
                    if f.read_to_string(&mut content).is_ok() {
                        header_data = serde_json::from_str::<CasePackageHeader>(&content).ok();
                    }
                    break;
                }
            }
        }
    }

    let header =
        header_data.ok_or_else(|| AmeleError::new(HataKodu::Genel, "Header dosyası bulunamadı"))?;

    let mut final_case_name = header.case_name.clone();
    let mut case_dir = target_base_dir.join(&final_case_name);
    if case_dir.exists() {
        final_case_name = format!("{}_imported", final_case_name);
        case_dir = target_base_dir.join(&final_case_name);
    }

    let tar_gz = File::open(package_path)
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Paket dosyası açılamadı", e))?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);

    let mut files_extracted = 0;
    if let Ok(entries) = archive.entries() {
        for mut file in entries.flatten() {
            let path = file.path().unwrap().into_owned();

            if path.to_str().unwrap_or_default() == "amele_case_header.json" {
                continue;
            }

            let rel_path = path
                .strip_prefix(&header.case_name)
                .unwrap_or(&path)
                .to_path_buf();
            let extract_path = case_dir.join(&rel_path);

            if let Some(parent) = extract_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if file.unpack(&extract_path).is_ok() && extract_path.is_file() {
                files_extracted += 1;
            }
        }
    }

    let mut integrity_verified = false;
    let mut warnings = vec![];
    let manifest_path = case_dir.join("case_manifest.json");
    if manifest_path.exists() {
        if let Ok(content) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) {
                integrity_verified = true;
                if let Some(files) = manifest["files"].as_array() {
                    for file in files {
                        if let (Some(rel), Some(hash)) =
                            (file["relative_path"].as_str(), file["sha256"].as_str())
                        {
                            let p = case_dir.join(rel);
                            if p.exists() {
                                if let Ok(calc) = calculate_file_hash(&p, HashAlgorithm::Sha256) {
                                    if !calc.eq_ignore_ascii_case(hash) {
                                        integrity_verified = false;
                                        warnings.push(format!("Hash uyuşmazlığı: {}", rel));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(ImportResult {
        case_name: final_case_name,
        case_dir,
        files_extracted,
        integrity_verified,
        warnings,
    })
}

pub fn verify_package(package_path: &Path) -> AmeleResult<bool> {
    let sidecar = package_path.with_extension(format!(
        "{}sha256",
        package_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default()
    ));

    if !sidecar.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(sidecar)
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Hash dosyası okunamadı", e))?;

    let expected_hash = content.split_whitespace().next().unwrap_or_default();
    let calc = calculate_file_hash(package_path, HashAlgorithm::Sha256)?;

    Ok(expected_hash.eq_ignore_ascii_case(&calc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_and_import_case() {
        let src_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();

        let vault = EvidenceVault::create(src_dir.path(), "testcase").unwrap();
        vault.add_note("test note").unwrap();
        fs::write(vault.new_file("ciktilar", "test.txt"), "hello").unwrap();

        let package = export_case(&vault, src_dir.path()).unwrap();
        assert!(package.exists());

        let import_res = import_case(&package, target_dir.path()).unwrap();
        assert!(import_res.files_extracted >= 3);
        assert!(import_res.integrity_verified);
    }

    #[test]
    fn test_verify_package_integrity() {
        let src_dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(src_dir.path(), "testcase2").unwrap();
        let package = export_case(&vault, src_dir.path()).unwrap();

        assert!(verify_package(&package).unwrap());
    }
}
