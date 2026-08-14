use crate::case_package::{export_case, import_case, verify_package};
use crate::evidence::EvidenceVault;
use crate::server::{Response, json_error, json_ok, json_serialize};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct ExportRequest {
    case_name: String,
    output_path: String,
}

#[derive(Deserialize)]
struct PackagePathRequest {
    package_path: String,
}

fn parse_body<'de, T: Deserialize<'de>>(body: &'de [u8]) -> Result<T, Response> {
    serde_json::from_slice(body).map_err(|e| json_error(400, e.to_string()))
}

pub fn case_export_endpoint(body: &[u8]) -> Response {
    let req: ExportRequest = match parse_body(body) {
        Ok(r) => r,
        Err(r) => return r,
    };
    let base_dir = crate::api::default_case_base_dir();
    let vault = match EvidenceVault::create(&base_dir, &req.case_name) {
        Ok(v) => v,
        Err(e) => return json_error(500, e.to_string()),
    };
    match export_case(&vault, &PathBuf::from(req.output_path)) {
        Ok(path) => {
            let mut hash_path = path.as_os_str().to_owned();
            hash_path.push(".sha256");
            json_ok(serde_json::json!({
                "ok": true,
                "package_path": path,
                "hash_path": PathBuf::from(hash_path)
            }))
        }
        Err(e) => json_error(500, e.to_string()),
    }
}

pub fn case_import_endpoint(body: &[u8]) -> Response {
    let req: PackagePathRequest = match parse_body(body) {
        Ok(r) => r,
        Err(r) => return r,
    };
    let base_dir = crate::api::default_case_base_dir();
    match import_case(&PathBuf::from(req.package_path), &base_dir) {
        Ok(res) => json_serialize(&res),
        Err(e) => json_error(500, e.to_string()),
    }
}

pub fn case_verify_endpoint(body: &[u8]) -> Response {
    let req: PackagePathRequest = match parse_body(body) {
        Ok(r) => r,
        Err(r) => return r,
    };
    match verify_package(&PathBuf::from(req.package_path)) {
        Ok(verified) => json_ok(serde_json::json!({ "ok": true, "verified": verified })),
        Err(e) => json_error(500, e.to_string()),
    }
}
