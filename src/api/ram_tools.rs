//! RAM edinim araçlarının indirme ve kurulum API uçlarını içerir.
use serde_json::{Value, json};
use std::fs;

use crate::ram;
use crate::server::{Response, json_error, json_ok};

use super::{
    cleanup_helper_files, download_file_to_path, helper_file_stem, read_helper_json,
    run_elevated_helper_wait, sha256_file,
};

#[cfg(windows)]
const WINPMEM_DOWNLOAD_URL: &str = "https://amele.noirlang.tr/go-winpmem_amd64_1.0-rc2_signed.exe";

#[cfg(windows)]
use super::{
    create_acquisition_job, fail_acquisition_job_with_message, finish_acquisition_job_with_message,
    update_acquisition_message, update_acquisition_progress_message,
};

/// Linux için AVML indirme/kurulum işini başlatır.
pub fn avml_install_endpoint() -> Response {
    #[cfg(not(target_os = "linux"))]
    {
        return json_error(400, "AVML installation is only supported on Linux");
    }

    #[cfg(target_os = "linux")]
    {
        let Some(asset_name) = avml_release_asset_name() else {
            return json_error(
                400,
                format!(
                    "AVML binary is not available for this architecture: {}",
                    std::env::consts::ARCH
                ),
            );
        };
        let url =
            format!("https://github.com/microsoft/avml/releases/latest/download/{asset_name}");
        let download_dir = std::env::temp_dir().join("amele-avml-install");
        if let Err(err) = fs::create_dir_all(&download_dir) {
            return json_error(500, err.to_string());
        }
        let download_path = download_dir.join(format!("{asset_name}.download"));

        if let Err(err) = download_file_to_path(&url, &download_path, "AVML download failed") {
            let _ = fs::remove_file(&download_path);
            return json_error(500, err);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&download_path) {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                let _ = fs::set_permissions(&download_path, permissions);
            }
        }

        let sha256 = match sha256_file(&download_path) {
            Ok(value) => value,
            Err(err) => {
                let _ = fs::remove_file(&download_path);
                return json_error(500, err);
            }
        };
        let stem = helper_file_stem("amele-avml-install");
        let result_path = download_dir.join(format!("{stem}-result.json"));
        let args = vec![
            "avml-install-helper".to_string(),
            download_path.to_string_lossy().into_owned(),
            result_path.to_string_lossy().into_owned(),
        ];

        let run_result = run_elevated_helper_wait(&args);
        let helper_result = read_helper_json(&result_path).ok();
        cleanup_helper_files(&[&download_path, &result_path]);
        if let Err(err) = run_result {
            let message = helper_result
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(Value::as_str)
                .unwrap_or(&err)
                .to_string();
            return json_error(500, message);
        }

        let helper_result = match helper_result {
            Some(value) => value,
            None => return json_error(500, "AVML install helper did not return a result"),
        };
        if helper_result.get("ok").and_then(Value::as_bool) != Some(true) {
            return json_error(
                500,
                helper_result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("AVML installation failed")
                    .to_string(),
            );
        }

        json_ok(json!({
            "asset": asset_name,
            "download_url": url,
            "sha256": sha256,
            "path": helper_result.get("path").cloned().unwrap_or(Value::Null),
            "version": helper_result.get("version").cloned().unwrap_or(Value::Null),
            "message": helper_result.get("message").cloned().unwrap_or(Value::String("AVML installed".to_string())),
            "status": ram::avml_status(None),
        }))
    }
}

/// Windows için WinPMEM indirme/kurulum işini başlatır.
pub fn winpmem_install_endpoint() -> Response {
    #[cfg(not(windows))]
    {
        return json_error(400, "WinPMEM installation is only supported on Windows");
    }

    #[cfg(windows)]
    {
        if std::env::consts::ARCH != "x86_64" {
            return json_error(
                400,
                format!(
                    "WinPMEM binary is not available for this architecture: {}",
                    std::env::consts::ARCH
                ),
            );
        }

        let (job_id, _control) = create_acquisition_job("WinPMEM indiriliyor");
        let thread_job_id = job_id.clone();
        std::thread::spawn(move || run_winpmem_install_job(thread_job_id));

        json_ok(json!({
            "job_id": job_id,
            "status": "running",
            "message": "WinPMEM indirme başlatıldı",
        }))
    }
}

#[cfg(windows)]
/// WinPMEM indirme/kurulum işini arka planda çalıştırır.
fn run_winpmem_install_job(job_id: String) {
    update_acquisition_message(&job_id, "WinPMEM indiriliyor...");

    let download_dir = std::env::temp_dir().join("amele-winpmem-install");
    if let Err(err) = fs::create_dir_all(&download_dir) {
        fail_acquisition_job_with_message(&job_id, err.to_string(), "WinPMEM indirme başarısız");
        return;
    }
    let download_path = download_dir.join(ram::WINPMEM_NAME);

    let monitor_job_id = job_id.clone();
    let monitor_path = download_path.clone();
    let total_expected_bytes = 3_831_296; // ~3.65 MB
    let monitor_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let thread_stop = monitor_stop.clone();

    let monitor_thread = std::thread::spawn(move || {
        while !thread_stop.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok(metadata) = fs::metadata(&monitor_path) {
                let size = metadata.len();
                let pct = (size * 100)
                    .checked_div(total_expected_bytes)
                    .unwrap_or(0)
                    .min(100);
                update_acquisition_progress_message(
                    &monitor_job_id,
                    size,
                    total_expected_bytes,
                    &format!("WinPMEM indiriliyor... %{pct}"),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });

    let download_result = download_file_to_path(
        WINPMEM_DOWNLOAD_URL,
        &download_path,
        "WinPMEM download failed",
    );

    monitor_stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = monitor_thread.join();

    if let Err(err) = download_result {
        let _ = fs::remove_file(&download_path);
        fail_acquisition_job_with_message(&job_id, err, "WinPMEM indirme başarısız");
        return;
    }

    update_acquisition_message(&job_id, "WinPMEM SHA256 hesaplanıyor...");
    let sha256 = match sha256_file(&download_path) {
        Ok(value) => value,
        Err(err) => {
            let _ = fs::remove_file(&download_path);
            fail_acquisition_job_with_message(&job_id, err, "WinPMEM hash hesaplama başarısız");
            return;
        }
    };

    update_acquisition_message(&job_id, "WinPMEM kuruluşu yapılıyor (yetki gerekli)...");
    let stem = helper_file_stem("amele-winpmem-install");
    let result_path = download_dir.join(format!("{stem}-result.json"));
    let args = vec![
        "winpmem-install-helper".to_string(),
        download_path.to_string_lossy().into_owned(),
        result_path.to_string_lossy().into_owned(),
    ];

    let run_result = run_elevated_helper_wait(&args);
    let helper_result = read_helper_json(&result_path).ok();
    cleanup_helper_files(&[&download_path, &result_path]);

    if let Err(err) = run_result {
        let message = helper_result
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(Value::as_str)
            .unwrap_or(&err)
            .to_string();
        fail_acquisition_job_with_message(&job_id, message, "WinPMEM kurulum başarısız");
        return;
    }

    let helper_result = match helper_result {
        Some(value) => value,
        None => {
            fail_acquisition_job_with_message(
                &job_id,
                "WinPMEM install helper did not return a result".to_string(),
                "WinPMEM kurulum başarısız",
            );
            return;
        }
    };

    if helper_result.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = helper_result
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("WinPMEM installation failed")
            .to_string();
        fail_acquisition_job_with_message(&job_id, message, "WinPMEM kurulum başarısız");
        return;
    }

    finish_acquisition_job_with_message(
        &job_id,
        json!({
            "asset": ram::WINPMEM_NAME,
            "download_url": WINPMEM_DOWNLOAD_URL,
            "sha256": sha256,
            "path": helper_result.get("path").cloned().unwrap_or(Value::Null),
            "message": helper_result.get("message").cloned().unwrap_or(Value::String("WinPMEM installed".to_string())),
            "status": ram::winpmem_status(None),
        }),
        "WinPMEM kuruldu",
    );
}

/// Linux dağıtım mimarisine uygun AVML release asset adını döndürür.
fn avml_release_asset_name() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("avml"),
        "aarch64" => Some("avml-aarch64"),
        _ => None,
    }
}
