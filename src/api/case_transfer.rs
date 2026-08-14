use crate::case_package::{export_case, import_case, verify_package};
use crate::evidence::EvidenceVault;
use crate::server::{Response, json_error, json_ok};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct ExportRequest {
    case_name: String,
    output_path: String,
}

#[derive(Deserialize)]
struct ImportRequest {
    package_path: String,
}

#[derive(Deserialize)]
struct VerifyRequest {
    package_path: String,
}

pub fn case_export_endpoint(body: &[u8]) -> Response {
    let req: ExportRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => return json_error(400, "JSON ayrıştırma hatası"),
    };

    let base_dir = crate::api::default_case_base_dir();

    let vault = match EvidenceVault::create(&base_dir, &req.case_name) {
        Ok(v) => v,
        Err(e) => return json_error(500, &format!("Vaka açılamadı: {:?}", e)),
    };

    match export_case(&vault, &PathBuf::from(req.output_path)) {
        Ok(path) => {
            let hash_path = path.with_extension(format!(
                "{}sha256",
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!("{ext}."))
                    .unwrap_or_default()
            ));
            json_ok(serde_json::json!({
                "ok": true,
                "package_path": path,
                "hash_path": hash_path
            }))
        }
        Err(e) => json_error(500, &format!("Dışa aktarma hatası: {:?}", e)),
    }
}

pub fn case_import_endpoint(body: &[u8]) -> Response {
    let req: ImportRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => return json_error(400, "JSON ayrıştırma hatası"),
    };

    let base_dir = crate::api::default_case_base_dir();

    match import_case(&PathBuf::from(req.package_path), &base_dir) {
        Ok(res) => json_ok(serde_json::to_value(res).unwrap()),
        Err(e) => json_error(500, &format!("İçe aktarma hatası: {:?}", e)),
    }
}

pub fn case_verify_endpoint(body: &[u8]) -> Response {
    let req: VerifyRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => return json_error(400, "JSON ayrıştırma hatası"),
    };

    match verify_package(&PathBuf::from(req.package_path)) {
        Ok(verified) => json_ok(serde_json::json!({
            "ok": true,
            "verified": verified
        })),
        Err(e) => json_error(500, &format!("Doğrulama hatası: {:?}", e)),
    }
}
