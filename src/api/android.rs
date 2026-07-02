//! Android edinim ve analiz işlemlerinin HTTP API uçlarını sağlar.
use crate::android;
use crate::android_analysis;
use crate::api::{
    create_acquisition_job, evidence_vault_for_output, fail_acquisition_job_with_message,
    finish_acquisition_job_with_message, report_evidence_vault, sanitize_file_stem,
    update_acquisition_progress_message,
};
use crate::ram;
use crate::server::{Response, json_error, json_ok};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::thread;

/// Seçilen vakanın Android çıktılarından analiz özeti üretir.
pub fn android_case_analysis_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidAnalysisRequest {
        case_name: Option<String>,
    }

    let request: AndroidAnalysisRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let vault = match report_evidence_vault(request.case_name.as_deref()) {
        Ok(vault) => vault,
        Err(response) => return response,
    };
    let report = android_analysis::analyze_android_case(&vault.case_name, &vault.android_dir);
    json_ok(serde_json::to_value(report).unwrap_or(serde_json::Value::Null))
}

/// Bağlı Android cihazın model, API, root ve şifreleme profilini döndürür.
pub fn android_device_profile_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidProfileRequest {
        serial: String,
    }

    let request: AndroidProfileRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    match android::detect_device_profile(serial) {
        Ok(profile) => {
            let session = android::build_android_session(serial, profile.clone());
            let capabilities = android::build_android_capability_report(serial, &profile);
            json_ok(json!({
                "profile": profile,
                "session": session,
                "capabilities": capabilities,
            }))
        }
        Err(err) => json_error(500, android::explain_android_error(err)),
    }
}

/// Android profil/mantıksal edinim işini arka planda başlatır.
pub fn android_profile_acquisition_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidProfileAcquisitionRequest {
        serial: String,
        case_name: Option<String>,
        profile: Option<String>,
    }

    let request: AndroidProfileAcquisitionRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim().to_string();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }
    let profile = request
        .profile
        .as_deref()
        .map(android::AndroidAcquisitionProfile::from_id)
        .unwrap_or(android::AndroidAcquisitionProfile::FullLogical);

    let (job_id, control) = create_acquisition_job("Android profil edinimi baslatildi");
    let thread_job_id = job_id.clone();
    thread::spawn(move || {
        run_android_profile_acquisition_job(
            thread_job_id,
            serial,
            request.case_name,
            profile,
            control,
        )
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

fn run_android_profile_acquisition_job(
    job_id: String,
    serial: String,
    case_name: Option<String>,
    profile: android::AndroidAcquisitionProfile,
    control: ram::CancellationToken,
) {
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:profil",
        format!(
            "IS BASLADI | job_id={job_id} | serial={serial} | profil={:?} | vaka={}",
            profile,
            case_name.as_deref().unwrap_or("(otomatik)")
        ),
    );

    let vault = match evidence_vault_for_output(case_name.as_deref()) {
        Ok(vault) => vault,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:profil",
                format!(
                    "IS BASARISIZ (vaka hatasi) | job_id={job_id} | serial={serial} | hata={err}"
                ),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android profil edinimi basarisiz");
            return;
        }
    };

    let android_dir = match android_edinim_klasoru(&vault.android_dir, "profile", &serial) {
        Ok(path) => path,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:profil",
                format!(
                    "IS BASARISIZ (klasor hatasi) | job_id={job_id} | serial={serial} | hata={err}"
                ),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android profil edinimi basarisiz");
            return;
        }
    };

    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:profil",
        format!(
            "Cikti dizini hazir: {:?} | job_id={job_id} | serial={serial}",
            android_dir
        ),
    );

    if let Err(err) = std::fs::create_dir_all(&android_dir) {
        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:profil",
            format!("IS BASARISIZ (dizin olusturulamadi) | job_id={job_id} | hata={err}"),
        );
        fail_acquisition_job_with_message(
            &job_id,
            err.to_string(),
            "Android profil edinimi basarisiz",
        );
        return;
    }

    match android::orchestrated_acquisition(
        &serial,
        &android_dir,
        profile,
        |done, total, category| {
            update_acquisition_progress_message(
                &job_id,
                done as u64,
                total as u64,
                &format!("Toplaniyor: {category}"),
            );
        },
        || android_job_should_stop(&control),
    ) {
        Ok(result) => {
            let success_count = result.items.iter().filter(|i| i.success).count();
            let fail_count = result.items.iter().filter(|i| !i.success).count();
            let total_count = result.items.len();
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "android:profil",
                format!(
                    "IS TAMAMLANDI | job_id={job_id} | serial={serial} | {success_count}/{total_count} adim basarili | {fail_count} basarisiz | {} byte | cikti={:?}",
                    result.total_bytes, result.output_dir,
                ),
            );
            if !result.errors.is_empty() {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Warn,
                    "android:profil",
                    format!("IS HATALARI | job_id={job_id} | {:?}", result.errors),
                );
            }
            finish_acquisition_job_with_message(
                &job_id,
                json!({
                    "message": format!("Android profil edinimi tamamlandi ({success_count}/{total_count} adim basarili)"),
                    "profile": result.profile,
                    "device_profile": result.device_profile,
                    "session": result.session,
                    "capabilities": result.capabilities,
                    "output_dir": result.output_dir,
                    "total_bytes": result.total_bytes,
                    "sha256": result.sha256,
                    "manifest_sha256": result.manifest_sha256,
                    "items": result.items,
                    "errors": result.errors,
                }),
                "Android profil edinimi tamamlandi",
            );
        }
        Err(err) => {
            let explained = android::explain_android_error(err.clone());
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:profil",
                format!(
                    "IS BASARISIZ | job_id={job_id} | serial={serial} | ham_hata={err} | aciklama={explained}"
                ),
            );
            fail_android_job(&job_id, err, "Android profil edinimi basarisiz");
        }
    }
}

/// Eski mantıksal imaj endpoint'ini arka plan işi olarak çalıştırır.
pub fn android_logical_image_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidLogicalRequest {
        serial: String,
        case_name: Option<String>,
    }

    let request: AndroidLogicalRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim().to_string();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    let (job_id, control) = create_acquisition_job("Android mantiksal imaj alma baslatildi");
    let thread_job_id = job_id.clone();
    thread::spawn(move || {
        run_android_logical_job(thread_job_id, serial, request.case_name, control)
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

fn run_android_logical_job(
    job_id: String,
    serial: String,
    case_name: Option<String>,
    control: ram::CancellationToken,
) {
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:logical",
        format!(
            "IS BASLADI | job_id={job_id} | serial={serial} | vaka={}",
            case_name.as_deref().unwrap_or("(otomatik)")
        ),
    );

    let vault = match evidence_vault_for_output(case_name.as_deref()) {
        Ok(vault) => vault,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:logical",
                format!("IS BASARISIZ (vaka hatasi) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android imaj alma basarisiz");
            return;
        }
    };

    let android_dir = match android_edinim_klasoru(&vault.android_dir, "logical", &serial) {
        Ok(path) => path,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:logical",
                format!("IS BASARISIZ (klasor hatasi) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android imaj alma basarisiz");
            return;
        }
    };

    if let Err(err) = std::fs::create_dir_all(&android_dir) {
        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:logical",
            format!("IS BASARISIZ (dizin) | job_id={job_id} | hata={err}"),
        );
        fail_acquisition_job_with_message(&job_id, err.to_string(), "Android imaj alma basarisiz");
        return;
    }

    match android::orchestrated_acquisition(
        &serial,
        &android_dir,
        android::AndroidAcquisitionProfile::FullLogical,
        |done, total, category| {
            update_acquisition_progress_message(
                &job_id,
                done as u64,
                total as u64,
                &format!("Toplaniyor: {category}"),
            );
        },
        || android_job_should_stop(&control),
    ) {
        Ok(result) => {
            let success_count = result.items.iter().filter(|i| i.success).count();
            let fail_count = result.items.iter().filter(|i| !i.success).count();
            let total_count = result.items.len();
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "android:logical",
                format!(
                    "IS TAMAMLANDI | job_id={job_id} | serial={serial} | {success_count}/{total_count} basarili | {fail_count} basarisiz | {} byte | cikti={:?}",
                    result.total_bytes, result.output_dir
                ),
            );
            if !result.errors.is_empty() {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Warn,
                    "android:logical",
                    format!("IS HATALARI | job_id={job_id} | {:?}", result.errors),
                );
            }
            finish_acquisition_job_with_message(
                &job_id,
                json!({
                    "message": format!("Android mantiksal imaj tamamlandi ({success_count}/{total_count} adim basarili)"),
                    "profile": result.profile,
                    "device_profile": result.device_profile,
                    "session": result.session,
                    "capabilities": result.capabilities,
                    "output_dir": result.output_dir,
                    "total_bytes": result.total_bytes,
                    "sha256": result.sha256,
                    "manifest_sha256": result.manifest_sha256,
                    "items": result.items,
                    "errors": result.errors,
                }),
                "Android mantiksal imaj tamamlandi",
            );
        }
        Err(err) => {
            let explained = android::explain_android_error(err.clone());
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:logical",
                format!(
                    "IS BASARISIZ | job_id={job_id} | serial={serial} | ham_hata={err} | aciklama={explained}"
                ),
            );
            fail_android_job(&job_id, err, "Android imaj alma basarisiz");
        }
    }
}

/// Android dosya sistemi edinim işini başlatır.
pub fn android_filesystem_image_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidFilesystemRequest {
        serial: String,
        case_name: Option<String>,
        has_root: Option<bool>,
    }

    let request: AndroidFilesystemRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim().to_string();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    let (job_id, control) = create_acquisition_job("Android dosya sistemi imaj alma baslatildi");
    let thread_job_id = job_id.clone();
    let has_root = request.has_root.unwrap_or(false);
    thread::spawn(move || {
        run_android_filesystem_job(thread_job_id, serial, request.case_name, has_root, control)
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

fn run_android_filesystem_job(
    job_id: String,
    serial: String,
    case_name: Option<String>,
    has_root: bool,
    control: ram::CancellationToken,
) {
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:filesystem",
        format!(
            "IS BASLADI | job_id={job_id} | serial={serial} | root={has_root} | vaka={}",
            case_name.as_deref().unwrap_or("(otomatik)")
        ),
    );

    let vault = match evidence_vault_for_output(case_name.as_deref()) {
        Ok(vault) => vault,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:filesystem",
                format!("IS BASARISIZ (vaka) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(
                &job_id,
                err,
                "Android dosya sistemi imaj alma basarisiz",
            );
            return;
        }
    };

    let android_dir = match android_edinim_klasoru(&vault.android_dir, "filesystem", &serial) {
        Ok(path) => path,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:filesystem",
                format!("IS BASARISIZ (klasor) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(
                &job_id,
                err,
                "Android dosya sistemi imaj alma basarisiz",
            );
            return;
        }
    };
    if let Err(err) = std::fs::create_dir_all(&android_dir) {
        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:filesystem",
            format!("IS BASARISIZ (dizin) | job_id={job_id} | hata={err}"),
        );
        fail_acquisition_job_with_message(
            &job_id,
            err.to_string(),
            "Android dosya sistemi imaj alma basarisiz",
        );
        return;
    }

    match android::orchestrated_filesystem_acquisition(
        &serial,
        &android_dir,
        has_root,
        |done, total, category| {
            update_acquisition_progress_message(&job_id, done as u64, total as u64, category);
        },
        || android_job_should_stop(&control),
    ) {
        Ok(result) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "android:filesystem",
                format!(
                    "IS TAMAMLANDI | job_id={job_id} | serial={serial} | {} byte | cikti={:?}",
                    result.total_bytes, result.output_file
                ),
            );
            finish_acquisition_job_with_message(
                &job_id,
                json!({
                    "message": "Android dosya sistemi imajı başarıyla tamamlandı",
                    "device_profile": result.device_profile,
                    "session": result.session,
                    "capabilities": result.capabilities,
                    "output_file": result.output_file,
                    "total_bytes": result.total_bytes,
                    "sha256": result.sha256,
                    "manifest_sha256": result.manifest_sha256,
                }),
                "Android dosya sistemi imajı başarıyla tamamlandı",
            );
        }
        Err(err) => {
            let explained = android::explain_android_error(err.clone());
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:filesystem",
                format!(
                    "IS BASARISIZ | job_id={job_id} | serial={serial} | ham_hata={err} | aciklama={explained}"
                ),
            );
            fail_android_job(&job_id, err, "Android dosya sistemi imaj alma basarisiz");
        }
    }
}

/// Android uçucu veri/RAM odaklı edinim işini başlatır.
pub fn android_ram_image_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct AndroidRamRequest {
        serial: String,
        case_name: Option<String>,
        has_root: Option<bool>,
        mode: Option<String>,
    }

    let request: AndroidRamRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim().to_string();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    let (job_id, control) = create_acquisition_job("Android RAM imaj alma baslatildi");
    let thread_job_id = job_id.clone();
    let has_root = request.has_root.unwrap_or(false);
    let mode = request
        .mode
        .as_deref()
        .map(android::AndroidRamMode::from_id)
        .unwrap_or(android::AndroidRamMode::VolatileData);
    thread::spawn(move || {
        run_android_ram_job(
            thread_job_id,
            serial,
            request.case_name,
            has_root,
            mode,
            control,
        )
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

fn run_android_ram_job(
    job_id: String,
    serial: String,
    case_name: Option<String>,
    has_root: bool,
    mode: android::AndroidRamMode,
    control: ram::CancellationToken,
) {
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "android:ram",
        format!(
            "IS BASLADI | job_id={job_id} | serial={serial} | root={has_root} | mod={:?} | vaka={}",
            mode,
            case_name.as_deref().unwrap_or("(otomatik)")
        ),
    );

    let vault = match evidence_vault_for_output(case_name.as_deref()) {
        Ok(vault) => vault,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:ram",
                format!("IS BASARISIZ (vaka) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android RAM imaj alma basarisiz");
            return;
        }
    };

    let android_dir = match android_edinim_klasoru(&vault.android_dir, "ram", &serial) {
        Ok(path) => path,
        Err(err) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:ram",
                format!("IS BASARISIZ (klasor) | job_id={job_id} | hata={err}"),
            );
            fail_acquisition_job_with_message(&job_id, err, "Android RAM imaj alma basarisiz");
            return;
        }
    };
    if let Err(err) = std::fs::create_dir_all(&android_dir) {
        crate::logging::runtime_log(
            crate::logging::LogLevel::Error,
            "android:ram",
            format!("IS BASARISIZ (dizin) | job_id={job_id} | hata={err}"),
        );
        fail_acquisition_job_with_message(
            &job_id,
            err.to_string(),
            "Android RAM imaj alma basarisiz",
        );
        return;
    }

    match android::orchestrated_ram_acquisition(
        &serial,
        &android_dir,
        has_root,
        mode,
        |done, total, category| {
            update_acquisition_progress_message(&job_id, done as u64, total as u64, category);
        },
        || android_job_should_stop(&control),
    ) {
        Ok(result) => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "android:ram",
                format!(
                    "IS TAMAMLANDI | job_id={job_id} | serial={serial} | mod={:?} | {} byte | cikti={:?}",
                    result.mode, result.total_bytes, result.output_file
                ),
            );
            finish_acquisition_job_with_message(
                &job_id,
                json!({
                    "message": "Android RAM imajı başarıyla tamamlandı",
                    "device_profile": result.device_profile,
                    "session": result.session,
                    "capabilities": result.capabilities,
                    "output_file": result.output_file,
                    "total_bytes": result.total_bytes,
                    "sha256": result.sha256,
                    "manifest_sha256": result.manifest_sha256,
                    "mode": result.mode,
                }),
                "Android RAM imajı başarıyla tamamlandı",
            );
        }
        Err(err) => {
            let explained = android::explain_android_error(err.clone());
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "android:ram",
                format!(
                    "IS BASARISIZ | job_id={job_id} | serial={serial} | ham_hata={err} | aciklama={explained}"
                ),
            );
            fail_android_job(&job_id, err, "Android RAM imaj alma basarisiz");
        }
    }
}

fn fail_android_job(job_id: &str, err: String, title: &str) {
    fail_acquisition_job_with_message(job_id, android::explain_android_error(err), title);
}

fn android_edinim_klasoru(
    android_kok_klasoru: &Path,
    edinim_turu: &str,
    serial: &str,
) -> Result<PathBuf, String> {
    let tarih = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let temiz_tur = sanitize_file_stem(edinim_turu);
    let temiz_serial = sanitize_file_stem(serial);
    let klasor_on_adi = if temiz_serial.is_empty() {
        format!("{temiz_tur}_{tarih}")
    } else {
        format!("{temiz_tur}_{temiz_serial}_{tarih}")
    };

    let mut bingol = klasor_on_adi.clone();
    let mut cikti_klasoru = android_kok_klasoru.join(&bingol);
    let mut tekrar = 1_u32;
    while cikti_klasoru.exists() {
        bingol = format!("{klasor_on_adi}_{tekrar}");
        cikti_klasoru = android_kok_klasoru.join(&bingol);
        tekrar += 1;
    }

    std::fs::create_dir_all(&cikti_klasoru)
        .map_err(|err| format!("Android edinim klasoru olusturulamadi: {err}"))?;
    Ok(cikti_klasoru)
}

fn android_job_should_stop(control: &ram::CancellationToken) -> bool {
    while control.is_paused() {
        if control.is_cancelled() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    control.is_cancelled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_edinim_klasoru_ayni_vakada_ciktilari_ayirir() {
        let temp = tempfile::tempdir().expect("temp dir");

        let ilk = android_edinim_klasoru(temp.path(), "logical", "emulator:5554").unwrap();
        let ikinci = android_edinim_klasoru(temp.path(), "logical", "emulator:5554").unwrap();

        assert_ne!(ilk, ikinci);
        assert!(ilk.is_dir());
        assert!(ikinci.is_dir());
        assert_eq!(ilk.parent(), Some(temp.path()));
        assert_eq!(ikinci.parent(), Some(temp.path()));
    }
}

// ---------------------------------------------------------------------------
// Lemon fiziksel RAM ön kontrol endpoint'i
// ---------------------------------------------------------------------------

/// Cihazın Lemon ile fiziksel RAM dump'ına uygunluğunu kontrol eder.
///
/// Senkron ve hızlı çalışır (~5-10s). ABI, root, eBPF/BTF, SoC uyarısı döner.
pub fn android_lemon_preflight_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct LemonPreflightRequest {
        serial: String,
    }

    let request: LemonPreflightRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    let report = android::lemon_preflight(serial);
    json_ok(serde_json::to_value(report).unwrap_or(serde_json::Value::Null))
}

// ---------------------------------------------------------------------------
// Remote ADB / MESH endpoint bağlantı yönetimi
// ---------------------------------------------------------------------------

/// Belirtilen host:port'a `adb connect` komutuyla bağlanır.
pub fn android_remote_connect_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct RemoteConnectRequest {
        host: String,
        port: Option<u16>,
        label: Option<String>,
        /// "tcp_adb" veya "mesh_relay"
        kind: Option<String>,
    }

    let request: RemoteConnectRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, err.to_string()),
    };
    let host = request.host.trim();
    if host.is_empty() {
        return json_error(400, "host is required");
    }

    let port = request.port.unwrap_or(5555);
    let kind = match request.kind.as_deref().unwrap_or("tcp_adb") {
        "mesh_relay" => android::RemoteEndpointKind::MeshRelay,
        _ => android::RemoteEndpointKind::TcpAdb,
    };

    let endpoint = android::RemoteAndroidEndpoint {
        label: request.label.unwrap_or_else(|| format!("{host}:{port}")),
        host: host.to_string(),
        port,
        kind,
    };

    let result = android::connect_remote_endpoint(&endpoint);
    json_ok(serde_json::to_value(&result).unwrap_or(serde_json::Value::Null))
}

/// Belirtilen serial'ı ADB cihaz listesinden çıkarır (`adb disconnect`).
pub fn android_remote_disconnect_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct RemoteDisconnectRequest {
        serial: String,
    }

    let request: RemoteDisconnectRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(err) => return json_error(400, err.to_string()),
    };
    let serial = request.serial.trim();
    if serial.is_empty() {
        return json_error(400, "serial is required");
    }

    let result = android::disconnect_remote_endpoint(serial);
    json_ok(serde_json::to_value(&result).unwrap_or(serde_json::Value::Null))
}

///iceman'le karşılaştığında donup kalmaktan başka ne yapabilirsinki silah sıkamazsın sokaklarda değilsin
///