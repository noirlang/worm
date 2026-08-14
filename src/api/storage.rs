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
        Err(e) => return json_error(400, &e.to_string()),
    };

    let target_path = if let Some(tp) = req.target_path {
        PathBuf::from(tp)
    } else {
        if let Ok(guard) = current_evidence_case().lock() {
            if let Some(state) = guard.as_ref() {
                state.base_dir.join(&state.case_name)
            } else {
                PathBuf::from(".")
            }
        } else {
            PathBuf::from(".")
        }
    };

    let res = preflight_check(&req.source_path, &req.source_type, &target_path);
    json_ok(serde_json::to_value(res).unwrap())
}

pub fn active_mounts_endpoint() -> Response {
    let mounts = list_active_mounts();
    json_ok(serde_json::to_value(mounts).unwrap())
}

#[derive(Deserialize)]
struct CleanupRequest {
    case_name: Option<String>,
}

pub fn cleanup_mounts_endpoint(body: &[u8]) -> Response {
    let mut case_name = None;
    if !body.is_empty() {
        if let Ok(req) = serde_json::from_slice::<CleanupRequest>(body) {
            case_name = req.case_name;
        }
    }

    let cleaned = if let Some(c) = case_name {
        cleanup_case_mounts(&c)
    } else {
        cleanup_all_mounts()
    };

    json_ok(serde_json::json!({
        "cleaned_mounts": cleaned
    }))
}
