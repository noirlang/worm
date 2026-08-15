//! Docker ve Konteyner Adli Bilişimi HTTP API uç noktalarını yönetir.
use chrono::Local;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;
use std::thread;

use crate::docker::{
    self, DockerAcquisitionRequest, check_docker_status, get_container_logs, list_containers,
};
use crate::evidence::EvidenceVault;
use crate::remote::RemoteConnection;
use crate::server::{Response, json_error, json_ok};

use super::{
    append_acquisition_log, create_acquisition_job, default_case_base_dir,
    fail_acquisition_job_with_message, finish_acquisition_job_with_message,
    update_acquisition_progress_message,
};

#[derive(Deserialize)]
/// Konteyner detay veya log talebi için gelen gövdeyi taşır.
pub struct ContainerTargetRequest {
    pub container_id: String,
    pub custom_docker_root: Option<String>,
    pub tail: Option<usize>,
}

#[derive(Deserialize)]
/// Uzak Docker bağlantı talebini taşır.
pub struct RemoteDockerRequest {
    pub ip: String,
    pub port: u16,
    pub token: Option<String>,
    pub container_id: Option<String>,
    pub tail: Option<usize>,
}

#[derive(Deserialize)]
/// Uzak Docker edinim talebini taşır.
pub struct RemoteDockerAcquisitionRequest {
    pub ip: String,
    pub port: u16,
    pub token: Option<String>,
    pub container_id: String,
    pub container_name: Option<String>,
    pub acquire_diff: Option<bool>,
    pub acquire_logs: Option<bool>,
    pub acquire_config: Option<bool>,
    pub case_name: Option<String>,
}

fn parse_docker_root_path(custom_root: Option<&str>) -> Option<&Path> {
    custom_root
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Path::new)
}

/// Yerel veya bağlanmış imajdaki Docker sistem durumunu döner.
pub fn docker_status_endpoint(custom_root: Option<&str>) -> Response {
    let path_opt = parse_docker_root_path(custom_root);
    let status = check_docker_status(path_opt);
    json_ok(json!({
        "durum": "ok",
        "status": status,
    }))
}

/// Yerel veya bağlanmış imajdaki Docker konteynerlerini listeler ve güvenlik analizini döner.
pub fn docker_containers_endpoint(custom_root: Option<&str>) -> Response {
    let path_opt = parse_docker_root_path(custom_root);
    match list_containers(path_opt) {
        Ok(containers) => json_ok(json!({
            "durum": "ok",
            "containers": containers,
            "toplam": containers.len(),
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Belirli bir konteynerin loglarını okur ve döner.
pub fn docker_logs_endpoint(body: &[u8]) -> Response {
    let req: ContainerTargetRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    let path_opt = parse_docker_root_path(req.custom_docker_root.as_deref());
    let tail = req.tail.unwrap_or(200);

    match get_container_logs(&req.container_id, tail, path_opt) {
        Ok(logs) => json_ok(json!({
            "durum": "ok",
            "container_id": req.container_id,
            "logs": logs,
            "toplam": logs.len(),
        })),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Yerel konteyner delillerini vaka klasörüne toplar (Arka planda iş başlatır).
pub fn docker_acquire_local_endpoint(body: &[u8]) -> Response {
    let req: DockerAcquisitionRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    let (job_id, _token) = create_acquisition_job(&format!(
        "Docker: {}",
        &req.container_id[..req.container_id.len().min(12)]
    ));

    let job_id_clone = job_id.clone();
    let container_id = req.container_id.clone();
    let base_dir = default_case_base_dir();

    thread::spawn(move || {
        append_acquisition_log(
            &job_id_clone,
            &format!("Yerel Docker edinimi başlatıldı: {}", container_id),
        );

        let res = docker::acquire_container_evidence(&req, &base_dir, |msg, done, total| {
            update_acquisition_progress_message(&job_id_clone, done, total, msg);
            append_acquisition_log(&job_id_clone, msg);
        });

        match res {
            Ok(result) => {
                append_acquisition_log(
                    &job_id_clone,
                    &format!(
                        "Docker edinimi tamamlandı. Vaka yolu: {} (SHA256: {})",
                        result.case_path,
                        result.diff_sha256.as_deref().unwrap_or("-")
                    ),
                );
                let result_json = serde_json::to_value(&result).unwrap_or(Value::Null);
                finish_acquisition_job_with_message(
                    &job_id_clone,
                    result_json,
                    &format!("Konteyner delilleri toplandı: {}", result.container_name),
                );
            }
            Err(err) => {
                append_acquisition_log(&job_id_clone, &format!("Docker edinim hatası: {}", err));
                fail_acquisition_job_with_message(
                    &job_id_clone,
                    err.to_string(),
                    "Docker edinim hatası",
                );
            }
        }
    });

    json_ok(json!({
        "durum": "ok",
        "is_id": job_id,
        "mesaj": "Docker edinim işi başlatıldı.",
    }))
}

/// Uzak agent'tan Docker durumunu çeker.
pub fn docker_remote_status_endpoint(body: &[u8]) -> Response {
    let req: RemoteDockerRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    match RemoteConnection::connect(&req.ip, req.port, req.token) {
        Ok(mut conn) => match conn.docker_status() {
            Ok(status) => json_ok(json!({
                "durum": "ok",
                "status": status,
            })),
            Err(err) => json_error(500, err.to_string()),
        },
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Uzak agent'taki Docker konteynerlerini listeler.
pub fn docker_remote_containers_endpoint(body: &[u8]) -> Response {
    let req: RemoteDockerRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    match RemoteConnection::connect(&req.ip, req.port, req.token) {
        Ok(mut conn) => match conn.list_docker_containers() {
            Ok(containers) => json_ok(json!({
                "durum": "ok",
                "containers": containers,
                "toplam": containers.len(),
            })),
            Err(err) => json_error(500, err.to_string()),
        },
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Uzak agent'taki belirli bir konteynerin loglarını çeker.
pub fn docker_remote_logs_endpoint(body: &[u8]) -> Response {
    let req: RemoteDockerRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    let container_id = match req.container_id {
        Some(id) => id,
        None => return json_error(400, "container_id parametresi gereklidir."),
    };

    match RemoteConnection::connect(&req.ip, req.port, req.token) {
        Ok(mut conn) => {
            match conn.get_docker_container_logs(&container_id, req.tail.unwrap_or(200)) {
                Ok(logs) => json_ok(json!({
                    "durum": "ok",
                    "container_id": container_id,
                    "logs": logs,
                    "toplam": logs.len(),
                })),
                Err(err) => json_error(500, err.to_string()),
            }
        }
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Uzak agent üzerindeki konteyner delillerini yerel vaka klasörüne indirir.
pub fn docker_remote_acquire_endpoint(body: &[u8]) -> Response {
    let req: RemoteDockerAcquisitionRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, format!("Gecersiz istek JSON: {}", err)),
    };

    let case_name = req.case_name.clone().unwrap_or_else(|| {
        format!(
            "VAKA_DOCKER_REMOTE_{}",
            Local::now().format("%Y%m%d_%H%M%S")
        )
    });

    let base_dir = default_case_base_dir();
    let vault = match EvidenceVault::create(&base_dir, &case_name) {
        Ok(v) => v,
        Err(err) => return json_error(500, format!("Vaka olusturulamadi: {}", err)),
    };

    let short_id = if req.container_id.len() >= 12 {
        &req.container_id[..12]
    } else {
        &req.container_id
    };

    let name = req.container_name.as_deref().unwrap_or("container");
    let target_dir = vault
        .case_dir
        .join("docker")
        .join(format!("{}_{}", name, short_id));

    let target_tar_path = target_dir.join("docker_evidence.tar.gz");

    let (job_id, _token) = create_acquisition_job(&format!("Uzak Docker: {} ({})", name, short_id));

    let job_id_clone = job_id.clone();
    let container_id = req.container_id.clone();

    thread::spawn(move || {
        append_acquisition_log(
            &job_id_clone,
            &format!(
                "Uzak Docker edinimi başlatıldı: {} ({}:{})",
                container_id, req.ip, req.port
            ),
        );

        let mut conn = match RemoteConnection::connect(&req.ip, req.port, req.token) {
            Ok(c) => c,
            Err(err) => {
                fail_acquisition_job_with_message(
                    &job_id_clone,
                    err.to_string(),
                    "Agent bağlantı hatası",
                );
                return;
            }
        };

        let acquire_diff = req.acquire_diff.unwrap_or(true);
        let acquire_logs = req.acquire_logs.unwrap_or(true);
        let acquire_config = req.acquire_config.unwrap_or(true);

        let job_ref = job_id_clone.clone();
        let res = conn.acquire_remote_docker(
            &container_id,
            acquire_diff,
            acquire_logs,
            acquire_config,
            &target_tar_path,
            Some(&job_id_clone),
            move |done, total| {
                update_acquisition_progress_message(
                    &job_ref,
                    done,
                    total,
                    "Uzak delil paketi aktarılıyor",
                );
            },
        );

        match res {
            Ok(result) => {
                let manifest_path = target_dir.join("manifest.csv");
                let manifest_content = format!(
                    "Dosya_Adi,Boyut_Byte,SHA256\ndocker_evidence.tar.gz,{},{}\n",
                    result.bytes_transferred,
                    result.sha256.as_deref().unwrap_or("HATA")
                );
                let _ = std::fs::write(&manifest_path, manifest_content);

                append_acquisition_log(
                    &job_id_clone,
                    &format!(
                        "Uzak Docker edinimi başarıyla tamamlandı. Dosya: {} (SHA256: {})",
                        result.target_path.display(),
                        result.sha256.as_deref().unwrap_or("-")
                    ),
                );
                let result_json = serde_json::to_value(&result).unwrap_or(Value::Null);
                finish_acquisition_job_with_message(
                    &job_id_clone,
                    result_json,
                    &format!(
                        "Uzak Docker delili aktarıldı ({} bytes)",
                        result.bytes_transferred
                    ),
                );
            }
            Err(err) => {
                append_acquisition_log(
                    &job_id_clone,
                    &format!("Uzak Docker edinim hatası: {}", err),
                );
                fail_acquisition_job_with_message(
                    &job_id_clone,
                    err.to_string(),
                    "Uzak Docker edinim hatası",
                );
            }
        }
    });

    json_ok(json!({
        "durum": "ok",
        "is_id": job_id,
        "mesaj": "Uzak Docker edinim işi başlatıldı.",
    }))
}
