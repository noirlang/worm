//! HTTP isteklerini API uçlarına veya statik UI dosyalarına yönlendirir.
use crate::api;
use crate::server::{self, Response};
use std::fs;

/// Gelen HTTP isteğini API router'a veya statik dosya sunucusuna yönlendirir.
pub fn route_request(method: &str, raw_path: &str, body: &[u8]) -> Response {
    if method == "OPTIONS" {
        return Response::empty(204);
    }

    let path = raw_path.split('?').next().unwrap_or("/");
    if path.starts_with("/api/") {
        return api::route_api(method, path, body);
    }

    if method != "GET" && method != "HEAD" {
        return server::json_error(405, "method not allowed");
    }

    serve_static(path, method == "HEAD")
}

/// UI asset dosyasını güvenli path kontrolüyle döndürür.
fn serve_static(path: &str, head_only: bool) -> Response {
    let path = if path == "/" { "/index.html" } else { path };
    let Ok(decoded) = server::percent_decode(path) else {
        return server::json_error(400, "invalid path encoding");
    };
    let relative = decoded.trim_start_matches('/');
    if relative.split('/').any(|part| part == "..") {
        return server::json_error(403, "path traversal rejected");
    }

    if let Some(root) = server::ui_root() {
        let mut file_path = root;
        file_path.push(relative);
        if file_path.is_dir() {
            file_path.push("index.html");
        }

        if let Ok(body) = fs::read(&file_path) {
            return Response {
                status: 200,
                content_type: server::mime_for(&file_path).to_string(),
                body: if head_only { Vec::new() } else { body },
            };
        }
    }

    if let Some(body) = server::embedded_ui_asset(relative) {
        let content_type = server::mime_for(std::path::Path::new(relative)).to_string();
        return Response {
            status: 200,
            content_type,
            body: if head_only { Vec::new() } else { body.to_vec() },
        };
    }

    Response {
        status: 404,
        content_type: "text/html; charset=utf-8".to_string(),
        body: b"Not found".to_vec(),
    }
}
