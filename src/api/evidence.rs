//! Vaka, kanıt notu, dosya listesi ve rapor API uçlarını yönetir.
use crate::api::{
    current_evidence_case, current_evidence_vault, default_case_base_dir, evidence_subdir,
    report_evidence_vault, sanitize_case_name, set_current_evidence_case,
};
use crate::evidence::{EvidenceVault, relative_case_path};
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
                "ios_dir": vault.ios_dir,
                "created_by": summary.created_by,
                "created_by_name": summary.created_by_name,
                "output_count": summary.output_count,
                "android_count": summary.android_count,
                "ios_count": summary.ios_count,
                "hash_count": summary.hash_count,
                "report_count": summary.report_count,
                "manifest_path": summary.manifest_path,
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
            "ios_count": summary.ios_count,
            "hash_count": summary.hash_count,
            "report_count": summary.report_count,
            "manifest_path": summary.manifest_path,
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Seçili veya aktif vaka için bütünlük manifesti üretir.
pub fn evidence_manifest_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct EvidenceManifestRequest {
        case_name: Option<String>,
    }

    let request: EvidenceManifestRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let vault = match report_evidence_vault(request.case_name.as_deref()) {
        Ok(vault) => vault,
        Err(response) => return response,
    };

    match vault.write_case_manifest() {
        Ok(path) => {
            let manifest = fs::read_to_string(&path)
                .ok()
                .and_then(|content| serde_json::from_str::<Value>(&content).ok());
            json_ok(json!({
                "path": path,
                "manifest": manifest,
            }))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Android ve iOS edinim manifestlerinden vaka geçmişini üretir.
pub fn acquisition_history_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AcquisitionHistoryRequest {
        case_name: Option<String>,
    }

    let request: AcquisitionHistoryRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let vault = match report_evidence_vault(request.case_name.as_deref()) {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    let history = acquisition_history_for_vault(&vault);
    json_ok(json!({
        "case_name": &vault.case_name,
        "case_dir": &vault.case_dir,
        "history": history,
    }))
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
                "ios_dir": case_dir.join("ios"),
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
        Ok(path) => {
            let manifest_path = vault.write_case_manifest().ok();
            json_ok(json!({
                "path": path,
                "manifest_path": manifest_path,
            }))
        }
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
        "ios_dir": case_dir.join("ios"),
        "created_by": metadata.created_by,
        "created_by_name": metadata.created_by_name,
        "created_at": metadata.created_at,
        "output_count": count_directory_entries(&case_dir.join("ciktilar")),
        "ram_count": count_directory_entries(&case_dir.join("ram")),
        "android_count": count_directory_entries(&case_dir.join("android")),
        "ios_count": count_directory_entries(&case_dir.join("ios")),
        "hash_count": count_directory_entries(&case_dir.join("hash")),
        "report_count": count_directory_entries(&case_dir.join("raporlar")),
        "manifest_path": case_dir.join("case_manifest.json"),
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

fn acquisition_history_for_vault(vault: &EvidenceVault) -> Vec<Value> {
    let mut items = Vec::new();
    collect_android_history(&vault.android_dir, &mut items);
    collect_ios_history(&vault.ios_dir, &mut items);
    collect_docker_history(&vault.docker_dir, &mut items);
    items.sort_by(|left, right| {
        right["sort_key"]
            .as_str()
            .unwrap_or_default()
            .cmp(left["sort_key"].as_str().unwrap_or_default())
    });
    items
}

fn collect_android_history(android_dir: &Path, items: &mut Vec<Value>) {
    let mut manifests = Vec::new();
    collect_named_files_recursive(android_dir, "android_manifest.json", &mut manifests);
    for manifest_path in manifests {
        let Some(item) = android_history_item(android_dir, &manifest_path) else {
            continue;
        };
        items.push(item);
    }
}

fn collect_ios_history(ios_dir: &Path, items: &mut Vec<Value>) {
    let mut manifests = Vec::new();
    collect_named_files_recursive(ios_dir, "ios_manifest.json", &mut manifests);
    for manifest_path in manifests {
        let Some(item) = ios_history_item(ios_dir, &manifest_path) else {
            continue;
        };
        items.push(item);
    }
}

fn collect_docker_history(docker_dir: &Path, items: &mut Vec<Value>) {
    let mut manifests = Vec::new();
    collect_named_files_recursive(docker_dir, "docker_metadata.json", &mut manifests);
    for meta_path in manifests {
        let Some(item) = docker_history_item(docker_dir, &meta_path) else {
            continue;
        };
        items.push(item);
    }
}

fn docker_history_item(docker_dir: &Path, meta_path: &Path) -> Option<Value> {
    let content = fs::read_to_string(meta_path).ok()?;
    let meta: Value = serde_json::from_str(&content).ok()?;
    let container_id = meta
        .get("konteyner_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let name = meta
        .get("isim")
        .and_then(|v| v.as_str())
        .unwrap_or("container");
    let time_str = meta
        .get("edinim_zamani")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let parent_dir = meta_path.parent()?;
    let rel_dir = relative_case_path(docker_dir, parent_dir);
    let short_id = if container_id.len() >= 12 {
        &container_id[..12]
    } else {
        container_id
    };
    let image = meta
        .get("config_v2")
        .and_then(|c| c.get("Config"))
        .and_then(|c| c.get("Image"))
        .and_then(|i| i.as_str())
        .unwrap_or("-");
    let bytes = meta.get("boyut").and_then(|v| v.as_u64()).unwrap_or(0);

    Some(json!({
        "id": format!("docker_{}", short_id),
        "platform": "docker",
        "title": format!("Docker: {} ({})", name, short_id),
        "subtitle": format!("İmaj: {}", image),
        "generated_at": time_str,
        "timestamp": time_str,
        "sort_key": time_str,
        "status": "completed",
        "total_bytes": bytes,
        "folder": parent_dir.to_string_lossy().to_string(),
        "relative_folder": rel_dir,
        "output_dir": parent_dir.to_string_lossy().to_string(),
        "relative_output": rel_dir,
        "manifest_path": meta_path.to_string_lossy().to_string(),
        "summary": {
            "container_id": container_id,
            "container_name": name,
            "image": image,
        }
    }))
}

fn android_history_item(android_dir: &Path, manifest_path: &Path) -> Option<Value> {
    let content = fs::read_to_string(manifest_path).ok()?;
    let manifest: Value = serde_json::from_str(&content).ok()?;
    let session = manifest.get("session").unwrap_or(&Value::Null);
    let profile = session.get("device_profile").unwrap_or(&Value::Null);
    let artifacts = manifest
        .get("artifacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let errors = manifest
        .get("errors")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let failed = artifacts
        .iter()
        .filter(|artifact| artifact.get("success").and_then(Value::as_bool) == Some(false))
        .count();
    let success = artifacts.len().saturating_sub(failed);
    let output_dir = manifest_path.parent().unwrap_or(android_dir);
    let relative_output = relative_path_string(android_dir, output_dir);
    let generated_at = json_string(&manifest, "generated_at")
        .or_else(|| json_string(session, "created_at"))
        .unwrap_or_else(|| file_modified_string(manifest_path));

    Some(json!({
        "id": format!("android:{relative_output}"),
        "platform": "android",
        "kind": json_string(&manifest, "acquisition_type").unwrap_or_else(|| "android".to_string()),
        "title": android_history_title(&manifest),
        "subtitle": android_device_label(session, profile),
        "generated_at": generated_at,
        "sort_key": file_modified_string(manifest_path),
        "output_dir": output_dir,
        "relative_output": relative_output,
        "manifest_path": manifest_path,
        "total_bytes": manifest.get("total_bytes").and_then(Value::as_u64).unwrap_or_default(),
        "success_count": success,
        "error_count": errors + failed,
        "status": if errors + failed == 0 { "completed" } else { "warnings" },
        "serial": json_string(session, "serial"),
        "model": json_string(profile, "model"),
        "manifest_sha256": json_string(&manifest, "acquisition_sha256"),
    }))
}

fn ios_history_item(ios_dir: &Path, manifest_path: &Path) -> Option<Value> {
    let content = fs::read_to_string(manifest_path).ok()?;
    let manifest: Value = serde_json::from_str(&content).ok()?;
    let device = manifest.get("device").unwrap_or(&Value::Null);
    let backup = manifest.get("backup").unwrap_or(&Value::Null);
    let summary = manifest.get("summary").unwrap_or(&Value::Null);
    let output_dir = manifest_path.parent().unwrap_or(ios_dir);
    let relative_output = relative_path_string(ios_dir, output_dir);
    let generated_at =
        json_string(&manifest, "created_at").unwrap_or_else(|| file_modified_string(manifest_path));
    let errors = json_usize(summary, "errors");
    let missing = json_usize(summary, "missing");

    Some(json!({
        "id": format!("ios:{relative_output}"),
        "platform": "ios",
        "kind": "ios_backup_normalize",
        "title": "iOS backup normalizasyonu",
        "subtitle": ios_device_label(device),
        "generated_at": generated_at,
        "sort_key": file_modified_string(manifest_path),
        "output_dir": output_dir,
        "relative_output": relative_output,
        "manifest_path": manifest_path,
        "total_bytes": summary.get("total_bytes").and_then(Value::as_u64).unwrap_or_default(),
        "total_entries": summary.get("total_entries").and_then(Value::as_u64).unwrap_or_default(),
        "files_copied": summary.get("files_copied").and_then(Value::as_u64).unwrap_or_default(),
        "success_count": summary.get("files_copied").and_then(Value::as_u64).unwrap_or_default(),
        "error_count": errors + missing,
        "status": if errors + missing == 0 { "completed" } else { "warnings" },
        "serial": json_string(device, "serial_number"),
        "model": json_string(device, "model"),
        "ios_version": json_string(device, "ios_version"),
        "encrypted": backup.get("encrypted").and_then(Value::as_bool).unwrap_or(false),
    }))
}

fn collect_named_files_recursive(dir: &Path, file_name: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named_files_recursive(&path, file_name, files);
        } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            files.push(path);
        }
    }
}

fn android_history_title(manifest: &Value) -> String {
    match json_string(manifest, "acquisition_type")
        .unwrap_or_default()
        .as_str()
    {
        "android_logical" => "Android mantiksal edinim".to_string(),
        "android_filesystem" => "Android dosya sistemi edinimi".to_string(),
        "android_ram" => "Android RAM edinimi".to_string(),
        value if !value.is_empty() => value.replace('_', " "),
        _ => "Android edinimi".to_string(),
    }
}

fn android_device_label(session: &Value, profile: &Value) -> String {
    let model = json_string(profile, "model")
        .or_else(|| json_string(profile, "product"))
        .unwrap_or_else(|| "Android cihaz".to_string());
    let serial = json_string(session, "serial").unwrap_or_else(|| "-".to_string());
    format!("{model} | {serial}")
}

fn ios_device_label(device: &Value) -> String {
    let model = json_string(device, "model")
        .or_else(|| json_string(device, "product_type"))
        .unwrap_or_else(|| "iOS cihaz".to_string());
    let version = json_string(device, "ios_version")
        .map(|value| format!("iOS {value}"))
        .unwrap_or_else(|| "iOS".to_string());
    format!("{model} | {version}")
}

fn relative_path_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn file_modified_string(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(|time| {
            chrono::DateTime::<Local>::from(time)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|item| !item.trim().is_empty())
}

fn json_usize(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| value.try_into().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_acquisition_history_from_android_and_ios_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::create(dir.path(), "history_case").unwrap();

        let android_run = vault.android_dir.join("logical_phone_20260727");
        fs::create_dir_all(&android_run).unwrap();
        fs::write(
            android_run.join("android_manifest.json"),
            serde_json::to_string_pretty(&json!({
                "acquisition_type": "android_logical",
                "generated_at": "2026-07-27T10:00:00+03:00",
                "session": {
                    "serial": "R5C123",
                    "created_at": "2026-07-27T10:00:00+03:00",
                    "device_profile": {
                        "model": "Pixel 8",
                        "product": "pixel"
                    }
                },
                "artifacts": [
                    { "success": true },
                    { "success": false }
                ],
                "total_bytes": 100,
                "errors": ["one failed"]
            }))
            .unwrap(),
        )
        .unwrap();

        let ios_run = vault.ios_dir.join("iphone_20260727");
        fs::create_dir_all(&ios_run).unwrap();
        fs::write(
            ios_run.join("ios_manifest.json"),
            serde_json::to_string_pretty(&json!({
                "created_at": "2026-07-27 11:00:00",
                "device": {
                    "model": "iPhone 15",
                    "ios_version": "18.5"
                },
                "backup": { "encrypted": false },
                "summary": {
                    "total_entries": 10,
                    "files_copied": 9,
                    "missing": 1,
                    "errors": 0,
                    "total_bytes": 200
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let docker_run = vault.docker_dir.join("web_nginx_a1b2c3d4e5f6");
        fs::create_dir_all(&docker_run).unwrap();
        fs::write(
            docker_run.join("docker_metadata.json"),
            serde_json::to_string_pretty(&json!({
                "edinim_zamani": "2026-07-27T12:00:00+03:00",
                "konteyner_id": "a1b2c3d4e5f67890abcdef123456",
                "isim": "web_nginx",
                "config_v2": {
                    "Config": { "Image": "nginx:alpine" }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let history = acquisition_history_for_vault(&vault);
        assert_eq!(history.len(), 3);
        assert!(history.iter().any(|item| item["platform"] == "android"));
        assert!(history.iter().any(|item| item["platform"] == "ios"));
        assert!(history.iter().any(|item| item["platform"] == "docker"));
    }
}
