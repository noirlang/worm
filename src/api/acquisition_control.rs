//! Disk/RAM edinim işlerinin durum ve kontrol API uçlarını yönetir.
use serde::Deserialize;
use serde_json::json;

use crate::disk;
use crate::remote::RemoteConnection;
use crate::server::{Response, json_error, json_ok};

use super::{AcquisitionJob, acquisition_jobs};

/// Edinim işinin canlı durumunu UI'ye döndürür.
pub fn acquisition_status_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct StatusRequest {
        job_id: String,
    }

    let request: StatusRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    match get_acquisition_job(&request.job_id) {
        Some(job) => json_ok(json!({
            "job_id": request.job_id,
            "status": job.status,
            "done": job.done,
            "total": job.total,
            "message": job.message,
            "logs": job.logs,
            "result": job.result,
            "error": job.error,
        })),
        None => json_error(404, "acquisition job not found"),
    }
}

/// Pause/resume/stop komutlarını ilgili edinim işine uygular.
pub fn acquisition_control_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct ControlRequest {
        ip: Option<String>,
        port: Option<u16>,
        token: Option<String>,
        job_id: String,
        action: String,
    }

    let request: ControlRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    if request.job_id.trim().is_empty() {
        return json_error(400, "job_id is required");
    }
    if !matches!(request.action.as_str(), "pause" | "resume" | "stop") {
        return json_error(400, "action must be pause, resume, or stop");
    }

    let ip = request.ip.unwrap_or_default();
    let remote_control = !ip.trim().is_empty();

    if remote_control {
        let port = request.port.unwrap_or_default();
        if port == 0 {
            return json_error(400, "port is required");
        }
        match RemoteConnection::connect(&ip, port, request.token) {
            Ok(connection) => match connection.control_job(&request.job_id, &request.action) {
                Ok(message) => {
                    apply_local_acquisition_control(&request.job_id, &request.action);
                    json_ok(json!({
                        "job_id": request.job_id,
                        "action": request.action,
                        "message": message,
                    }))
                }
                Err(err) => json_error(500, err.to_string()),
            },
            Err(err) => json_error(500, err.to_string()),
        }
    } else {
        match apply_local_acquisition_control(&request.job_id, &request.action) {
            Some(message) => json_ok(json!({
                "job_id": request.job_id,
                "action": request.action,
                "message": message,
            })),
            None => json_error(404, "acquisition job not found"),
        }
    }
}

/// Yerel edinim işine pause/resume/stop kontrolü uygular.
fn apply_local_acquisition_control(job_id: &str, action: &str) -> Option<String> {
    let mut jobs = acquisition_jobs().lock().ok()?;
    let job = jobs.get_mut(job_id)?;
    let (msg, ret) = match action {
        "pause" => {
            job.control.pause();
            job.status = "paused".to_string();
            ("Duraklatma komutu uygulandı", "Duraklatma komutu uygulandi")
        }
        "resume" => {
            job.control.resume();
            job.status = "running".to_string();
            ("Devam komutu uygulandı", "Devam komutu uygulandi")
        }
        "stop" => {
            job.control.cancel();
            disk::cancel_disk_acquisition();
            ("Durdurma komutu uygulandı", "Durdurma komutu uygulandi")
        }
        _ => return None,
    };
    job.message = msg.to_string();
    Some(ret.to_string())
}

/// Bellekte tutulan edinim işini ID ile döndürür.
fn get_acquisition_job(job_id: &str) -> Option<AcquisitionJob> {
    acquisition_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(job_id).cloned())
}
