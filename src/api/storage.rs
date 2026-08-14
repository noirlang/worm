use crate::api::current_evidence_case;
use crate::mount_tracker::{cleanup_all_mounts, cleanup_case_mounts, list_active_mounts};
use crate::server::{Response, json_error, json_ok};
use crate::storage_guard::preflight_check;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct PreflightRequest {
    source_path: String,
    source_type: String,
    target_path: Option<String>,
}

pub fn preflight_storage_check_endpoint(body: &[u8]) -> Response {
    let req: PreflightRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return json_error(400, e.to_string()),
    };

    let target_path = req
        .target_path
        .map(PathBuf::from)
        .or_else(|| {
            current_evidence_case()
                .lock()
                .ok()?
                .as_ref()
                .map(|s| s.base_dir.join(&s.case_name))
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let res = preflight_check(&req.source_path, &req.source_type, &target_path);
    match serde_json::to_value(res) {
        Ok(v) => json_ok(v),
        Err(e) => json_error(500, e.to_string()),
    }
}

pub fn active_mounts_endpoint() -> Response {
    match serde_json::to_value(list_active_mounts()) {
        Ok(v) => json_ok(v),
        Err(e) => json_error(500, e.to_string()),
    }
}

#[derive(Deserialize)]
struct CleanupRequest {
    case_name: Option<String>,
}

pub fn cleanup_mounts_endpoint(body: &[u8]) -> Response {
    let case_name = serde_json::from_slice::<CleanupRequest>(body)
        .ok()
        .and_then(|r| r.case_name);
    let cleaned = match case_name.as_deref() {
        Some(name) => cleanup_case_mounts(name),
        None => cleanup_all_mounts(),
    };
    json_ok(serde_json::json!({ "cleaned_mounts": cleaned }))
}
