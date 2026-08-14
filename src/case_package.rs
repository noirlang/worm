//! Vaka klasörlerini `.amelecase` (tar.gz) formatında dışa ve içe aktarır.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::evidence::EvidenceVault;
use crate::hash::{HashAlgorithm, calculate_file_hash, write_sha256_sidecar};
use chrono::Local;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};

const HEADER_ENTRY: &str = "amele_case_header.json";

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
pub struct ImportResult {
    pub case_name: String,
    pub case_dir: PathBuf,
    pub files_extracted: usize,
    pub integrity_verified: bool,
    pub warnings: Vec<String>,
}

fn count_files_recursive(dir: &Path) -> (usize, u64) {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .fold((0, 0), |(count, size), entry| {
            let Ok(meta) = entry.metadata() else {
                return (count, size);
            };
            if meta.is_file() {
                (count + 1, size + meta.len())
            } else if meta.is_dir() {
                let (c, s) = count_files_recursive(&entry.path());
                (count + c, size + s)
            } else {
                (count, size)
            }
        })
}

pub fn export_case(vault: &EvidenceVault, output_path: &Path) -> AmeleResult<PathBuf> {
    vault.write_case_manifest()?;

    let dest = if output_path.is_dir() {
        output_path.join(format!("{}.amelecase", vault.case_name))
    } else {
        output_path.to_path_buf()
    };

    let (file_count, total_bytes) = count_files_recursive(&vault.case_dir);
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

    let mut tar_header = tar::Header::new_gnu();
    tar_header.set_size(header_json.len() as u64);
    tar_header.set_mode(0o644);
    tar_header.set_cksum();

    let file = File::create(&dest)
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Paket dosyası oluşturulamadı", e))?;
    let mut builder = Builder::new(GzEncoder::new(file, Compression::default()));
    builder
        .append_data(&mut tar_header, HEADER_ENTRY, header_json.as_bytes())
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Header arşive eklenemedi", e))?;
    builder
        .append_dir_all(&vault.case_name, &vault.case_dir)
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Vaka dosyaları arşive eklenemedi", e))?;
    builder
        .into_inner()
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Tar arşivi kapatılamadı", e))?
        .finish()
        .map_err(|e| AmeleError::io(HataKodu::DosyaYazma, "Gz arşivi kapatılamadı", e))?;

    let hash = calculate_file_hash(&dest, HashAlgorithm::Sha256)?;
    write_sha256_sidecar(&dest, &hash)?;

    Ok(dest)
}

pub fn import_case(package_path: &Path, target_base_dir: &Path) -> AmeleResult<ImportResult> {
    // Arşivi iki kez okumak gerekiyor: önce header, sonra extract.
    // Streaming GzDecoder tek geçişe izin vermediği için iki ayrı File::open.
    let header = read_package_header(package_path)?;

    let mut case_dir = target_base_dir.join(&header.case_name);
    let mut final_case_name = header.case_name.clone();
    if case_dir.exists() {
        final_case_name = format!("{}_imported", header.case_name);
        case_dir = target_base_dir.join(&final_case_name);
    }

    let mut files_extracted = 0;
    let mut archive = open_archive(package_path)?;
    for mut entry in archive
        .entries()
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Arşiv okunamadı", e))?
        .flatten()
    {
        let path = entry
            .path()
            .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Dosya yolu okunamadı", e))?
            .into_owned();
        if path.to_str() == Some(HEADER_ENTRY) {
            continue;
        }
        let rel = path.strip_prefix(&header.case_name).unwrap_or(&path);
        let dest = case_dir.join(rel);
        if let Some(parent) = dest.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if entry.unpack(&dest).is_ok() && dest.is_file() {
            files_extracted += 1;
        }
    }

    let (integrity_verified, warnings) = verify_extracted_files(&case_dir);

    Ok(ImportResult {
        case_name: final_case_name,
        case_dir,
        files_extracted,
        integrity_verified,
        warnings,
    })
}

pub fn verify_package(package_path: &Path) -> AmeleResult<bool> {
    // Sidecar dosyası `foo.amelecase` -> `foo.amelecase.sha256`
    let sidecar = {
        let mut p = package_path.as_os_str().to_owned();
        p.push(".sha256");
        PathBuf::from(p)
    };
    if !sidecar.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&sidecar)
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Hash dosyası okunamadı", e))?;
    let expected = content.split_whitespace().next().unwrap_or_default();
    let actual = calculate_file_hash(package_path, HashAlgorithm::Sha256)?;
    Ok(expected.eq_ignore_ascii_case(&actual))
}

// ── private helpers ──────────────────────────────────────────────────────────

fn open_archive(path: &Path) -> AmeleResult<Archive<GzDecoder<File>>> {
    let file = File::open(path)
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Paket dosyası açılamadı", e))?;
    Ok(Archive::new(GzDecoder::new(file)))
}

fn read_package_header(package_path: &Path) -> AmeleResult<CasePackageHeader> {
    let mut archive = open_archive(package_path)?;
    archive
        .entries()
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "Arşiv okunamadı", e))?
        .flatten()
        .find(|e| {
            e.path()
                .map(|p| p.to_str() == Some(HEADER_ENTRY))
                .unwrap_or(false)
        })
        .ok_or_else(|| AmeleError::new(HataKodu::Genel, "Header dosyası bulunamadı"))
        .and_then(|mut e| {
            let mut buf = String::new();
            e.read_to_string(&mut buf)
                .map_err(|err| AmeleError::io(HataKodu::DosyaOkuma, "Header okunamadı", err))?;
            serde_json::from_str(&buf)
                .map_err(|err| AmeleError::new(HataKodu::Genel, &err.to_string()))
        })
}

fn verify_extracted_files(case_dir: &Path) -> (bool, Vec<String>) {
    let manifest_path = case_dir.join("case_manifest.json");
    let Ok(content) = fs::read_to_string(&manifest_path) else {
        return (false, vec![]);
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) else {
        return (false, vec![]);
    };
    let Some(files) = manifest["files"].as_array() else {
        return (false, vec![]);
    };

    let mut verified = true;
    let mut warnings = vec![];
    for file in files {
        let (Some(rel), Some(expected_hash)) =
            (file["relative_path"].as_str(), file["sha256"].as_str())
        else {
            continue;
        };
        let p = case_dir.join(rel);
        if !p.exists() {
            continue;
        }
        if let Ok(actual) = calculate_file_hash(&p, HashAlgorithm::Sha256) {
            if !actual.eq_ignore_ascii_case(expected_hash) {
                verified = false;
                warnings.push(format!("Hash uyuşmazlığı: {rel}"));
            }
        }
    }
    (verified, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_and_import_case() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(src.path(), "testcase").unwrap();
        vault.add_note("test note").unwrap();
        fs::write(vault.new_file("ciktilar", "test.txt"), "hello").unwrap();

        let pkg = export_case(&vault, src.path()).unwrap();
        assert!(pkg.exists());

        let res = import_case(&pkg, dst.path()).unwrap();
        assert!(res.files_extracted >= 3);
        assert!(res.integrity_verified);
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_verify_package_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "testcase2").unwrap();
        let pkg = export_case(&vault, dir.path()).unwrap();
        assert!(verify_package(&pkg).unwrap());
    }

    #[test]
    fn test_tampered_package_fails_verify() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "testcase3").unwrap();
        let pkg = export_case(&vault, dir.path()).unwrap();

        // Sidecar'ı olduğu gibi bırak, pakete veri ekle → hash uyuşmamalı
        let mut f = std::fs::OpenOptions::new().append(true).open(&pkg).unwrap();
        std::io::Write::write_all(&mut f, b"tampered").unwrap();
        assert!(!verify_package(&pkg).unwrap());
    }
}
