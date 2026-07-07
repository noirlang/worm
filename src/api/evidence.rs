//! Vaka, kanıt notu, dosya listesi ve rapor API uçlarını yönetir.
use crate::api::{
    current_evidence_case, current_evidence_vault, default_case_base_dir, evidence_subdir,
    report_evidence_vault, sanitize_case_name, set_current_evidence_case,
};
use crate::evidence::EvidenceVault;
use crate::report::{self, ReportFormat, ReportInfo};
use crate::server::{Response, json_error, json_ok};
use chrono::Local;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// Yeni vaka klasörü oluşturur ve aktif vakayı günceller.
pub fn evidence_create_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct EvidenceCreateRequest {
        case_name: String,
    }

    let request: EvidenceCreateRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    let case_name = sanitize_case_name(&request.case_name);
    if case_name.is_empty() {
        return json_error(400, "case_name is required");
    }
    let base_dir = default_case_base_dir();

    match EvidenceVault::create(&base_dir, &case_name) {
        Ok(vault) => {
            let summary = match vault.summary() {
                Ok(summary) => summary,
                Err(err) => return json_error(500, err.to_string()),
            };
            set_current_evidence_case(base_dir, case_name);
            json_ok(json!({
                "case_name": summary.case_name,
                "case_dir": summary.case_dir,
                "base_dir": default_case_base_dir(),
                "output_dir": vault.outputs_dir,
                "ram_dir": vault.ram_dir,
                "android_dir": vault.android_dir,
                "created_by": summary.created_by,
                "created_by_name": summary.created_by_name,
                "output_count": summary.output_count,
                "android_count": summary.android_count,
                "hash_count": summary.hash_count,
                "report_count": summary.report_count,
            }))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Aktif veya seçili vakaya metin notu ekler.
pub fn evidence_add_note_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct EvidenceNoteRequest {
        note: String,
        case_name: Option<String>,
    }

    let request: EvidenceNoteRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    if request.note.trim().is_empty() {
        return json_error(400, "note is required");
    }

    let vault = match report_evidence_vault(request.case_name.as_deref()) {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    match vault.add_note(request.note.trim()) {
        Ok(path) => json_ok(json!({ "path": path })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Aktif vaka alt klasöründeki dosyaları listeler.
pub fn evidence_list_files_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct EvidenceListRequest {
        subdir: Option<String>,
    }

    let request: EvidenceListRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let vault = match current_evidence_vault() {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    let subdir = evidence_subdir(request.subdir.as_deref().unwrap_or_default());

    match vault.list_files(subdir) {
        Ok(files) => {
            let files: Vec<Value> = files.into_iter().map(file_entry_json).collect();
            json_ok(json!({ "subdir": subdir, "files": files }))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Aktif vaka için dosya sayılarını döndürür.
pub fn evidence_summary_endpoint() -> Response {
    let vault = match current_evidence_vault() {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    match vault.summary() {
        Ok(summary) => json_ok(json!({
            "case_name": summary.case_name,
            "case_dir": summary.case_dir,
            "created_by": summary.created_by,
            "created_by_name": summary.created_by_name,
            "output_count": summary.output_count,
            "android_count": summary.android_count,
            "hash_count": summary.hash_count,
            "report_count": summary.report_count,
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Varsayılan vaka klasöründeki tüm vakaları listeler.
pub fn evidence_cases_endpoint() -> Response {
    let base_dir = default_case_base_dir();
    if let Err(err) = fs::create_dir_all(&base_dir) {
        return json_error(500, err.to_string());
    }

    let mut cases = Vec::new();
    let entries = match fs::read_dir(&base_dir) {
        Ok(entries) => entries,
        Err(err) => return json_error(500, err.to_string()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let case_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if case_name.is_empty() {
            continue;
        }
        cases.push(case_listing_json(&case_name, &path));
    }
    cases.sort_by(|left, right| {
        left["case_name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["case_name"].as_str().unwrap_or_default())
    });

    let current = current_evidence_case()
        .lock()
        .ok()
        .and_then(|state| state.clone())
        .map(|state| {
            let case_dir = state.base_dir.join(&state.case_name);
            json!({
                "case_name": state.case_name,
                "case_dir": case_dir,
                "base_dir": state.base_dir,
                "output_dir": case_dir.join("ciktilar"),
                "ram_dir": case_dir.join("ram"),
                "android_dir": case_dir.join("android"),
            })
        });

    json_ok(json!({
        "base_dir": base_dir,
        "cases": cases,
        "current_case": current,
    }))
}

/// Seçili vaka için TXT veya JSON rapor oluşturur.
pub fn report_create_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct ReportCreateRequest {
        case_name: Option<String>,
        title: Option<String>,
        description: Option<String>,
        source: Option<String>,
        hash_sha256: Option<String>,
        format: Option<String>,
    }

    let request: ReportCreateRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let vault = match report_evidence_vault(request.case_name.as_deref()) {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    let format = match report_format(request.format.as_deref().unwrap_or("txt")) {
        Some(format) => format,
        None => return json_error(400, "format must be txt or json"),
    };
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Forensic Technical Report");
    let description = request
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let source = request
        .source
        .as_deref()
        .map(str::trim)
        .unwrap_or("Amele Forensic Tool");
    let hash_sha256 = request
        .hash_sha256
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let info = ReportInfo {
        title: title.to_string(),
        description: description.to_string(),
        creator: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "amele".to_string()),
        source: source.to_string(),
        hash_sha256: hash_sha256.to_string(),
        date: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    let target = vault
        .reports_dir
        .join(report::new_report_file_name(&vault.case_name, format));

    match report::create_report(&info, format, &target, Some(&vault)) {
        Ok(path) => json_ok(json!({ "path": path })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Tek vaka klasörünü API listeleme JSON'una dönüştürür.
fn case_listing_json(case_name: &str, case_dir: &Path) -> Value {
    let metadata = crate::evidence::read_case_metadata(case_dir);
    json!({
        "case_name": case_name,
        "case_dir": case_dir,
        "output_dir": case_dir.join("ciktilar"),
        "ram_dir": case_dir.join("ram"),
        "android_dir": case_dir.join("android"),
        "created_by": metadata.created_by,
        "created_by_name": metadata.created_by_name,
        "created_at": metadata.created_at,
        "output_count": count_directory_entries(&case_dir.join("ciktilar")),
        "ram_count": count_directory_entries(&case_dir.join("ram")),
        "android_count": count_directory_entries(&case_dir.join("android")),
        "hash_count": count_directory_entries(&case_dir.join("hash")),
        "report_count": count_directory_entries(&case_dir.join("raporlar")),
    })
}

/// Klasördeki doğrudan girdi sayısını döndürür.
fn count_directory_entries(path: &Path) -> usize {
    fs::read_dir(path)
        .map(|entries| entries.flatten().count())
        .unwrap_or_default()
}

/// Dosya/klasör yolunu arayüzün beklediği JSON formata çevirir.
fn file_entry_json(path: PathBuf) -> Value {
    let metadata = fs::metadata(&path).ok();
    json!({
        "name": path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
        "path": path,
        "is_dir": metadata.as_ref().map(|meta| meta.is_dir()).unwrap_or(false),
        "size": metadata.as_ref().map(|meta| meta.len()).unwrap_or_default(),
    })
}

/// Rapor formatı stringini enum değerine çevirir.
fn report_format(value: &str) -> Option<ReportFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "txt" => Some(ReportFormat::Txt),
        "json" => Some(ReportFormat::Json),
        _ => None,
    }
}
