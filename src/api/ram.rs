//! Yerel/uzak RAM edinimi, araç kurulumu ve RAM analiz API uçlarını yönetir.
use chrono::Local;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use crate::hash::{self, HashAlgorithm};
use crate::ram;
use crate::ram_analysis;
use crate::remote::RemoteConnection;
use crate::server::{Response, json_error, json_ok};

const VOLATILITY_LINUX_BANNERS_URL: &str = "https://raw.githubusercontent.com/Abyss-W4tcher/volatility3-symbols/master/banners/banners_plain.json";
const VOLATILITY_LINUX_SYMBOL_RAW_BASE: &str =
    "https://github.com/Abyss-W4tcher/volatility3-symbols/raw/master/";

pub use super::ram_tools::{avml_install_endpoint, winpmem_install_endpoint};

use super::{
    append_acquisition_log,
    cleanup_helper_files,
    create_acquisition_job,
    current_evidence_vault,
    // Elevated and installer helpers
    download_file_to_path,
    evidence_vault_for_output,
    fail_acquisition_job_with_message,
    finish_acquisition_job_with_message,
    helper_file_stem,
    helper_owner_gid,
    helper_owner_uid,
    home_dir,
    process_is_root,
    read_helper_error,
    read_helper_json,
    read_helper_progress,
    sanitize_file_stem,
    sha256_file,
    spawn_elevated_helper,
    update_acquisition_message,
    update_acquisition_progress_message,
    write_helper_control_state,
    write_json_file,
};

#[derive(Deserialize)]
/// Yerel RAM edinim isteğinde araç, çıktı ve vaka bilgisini taşır.
pub struct LocalRamRequest {
    pub output: String,
    pub tool: Option<String>,
    pub tool_path: Option<String>,
    pub case_name: Option<String>,
}

#[derive(Deserialize)]
/// Uzak RAM edinim isteğinde agent bağlantısı, çıktı ve vaka bilgisini taşır.
pub struct RemoteRamRequest {
    pub ip: String,
    pub port: u16,
    pub token: Option<String>,
    pub output: String,
    pub case_name: Option<String>,
}

/// Yerel RAM edinim işini AVML veya WinPMEM ile başlatır.
pub fn local_ram_endpoint(body: &[u8]) -> Response {
    let request: LocalRamRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    if request.output.trim().is_empty() {
        return json_error(400, "output is required");
    }

    let tool = request.tool.as_deref().unwrap_or_default();
    if !matches!(tool, "avml" | "winpmem") {
        return json_error(400, "tool must be avml or winpmem");
    }

    let (job_id, control) = create_acquisition_job("Yerel RAM edinimi başlatıldı");
    let thread_job_id = job_id.clone();
    thread::spawn(move || run_local_ram_job(thread_job_id, request, control));

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

/// Uzak agent üzerinden RAM edinim işini başlatır.
pub fn remote_ram_endpoint(body: &[u8]) -> Response {
    let request: RemoteRamRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    if request.ip.trim().is_empty() {
        return json_error(400, "ip is required");
    }
    if request.port == 0 {
        return json_error(400, "port is required");
    }
    if request.output.trim().is_empty() {
        return json_error(400, "output is required");
    }

    let (job_id, _control) = create_acquisition_job("Uzak RAM edinimi başlatıldı");
    let thread_job_id = job_id.clone();
    thread::spawn(move || run_remote_ram_job(thread_job_id, request));

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
    }))
}

/// Yerel RAM edinim işini çalıştırır ve gerekirse yetkili helper fallback uygular.
fn run_local_ram_job(
    job_id: String,
    mut request: LocalRamRequest,
    control: ram::CancellationToken,
) {
    let output = match ram_output_path(&request.output, request.case_name.as_deref(), None) {
        Ok(output) => output,
        Err(err) => {
            fail_acquisition_job_with_message(&job_id, err, "RAM edinimi basarisiz");
            return;
        }
    };
    request.output = output.to_string_lossy().into_owned();

    let tool = request.tool.as_deref().unwrap_or_default();
    if local_ram_requires_elevation(tool) {
        run_elevated_local_ram_job(&job_id, &request, &control);
        return;
    }

    let output = PathBuf::from(&request.output);
    let candidate = request
        .tool_path
        .as_deref()
        .map(Path::new)
        .filter(|path| path.exists());

    let result = match tool {
        "avml" => ram::acquire_with_avml(&output, candidate, &control, |done, total| {
            update_acquisition_progress_message(&job_id, done, total, "RAM edinimi sürüyor");
        }),
        "winpmem" => ram::acquire_with_winpmem(&output, candidate, &control, |done, total| {
            update_acquisition_progress_message(&job_id, done, total, "RAM edinimi sürüyor");
        }),
        _ => Err(crate::error::AmeleError::new(
            crate::error::HataKodu::Genel,
            "Desteklenmeyen RAM araci",
        )),
    };

    match result {
        Ok(result) => {
            let sha256 = match finalize_ram_sha256(&job_id, &result.output_file, None) {
                Ok(value) => value,
                Err(err) => {
                    fail_acquisition_job_with_message(&job_id, err, "RAM hash olusturulamadi");
                    return;
                }
            };
            finish_acquisition_job_with_message(
                &job_id,
                json!({
                    "message": "RAM edinimi tamamlandi",
                    "target_path": result.output_file,
                    "bytes_written": result.bytes_written,
                    "sha256": sha256,
                }),
                "RAM edinimi tamamlandi",
            );
        }
        Err(err) => {
            let message = err.to_string();
            if local_ram_error_can_retry_elevated(&message) {
                run_elevated_local_ram_job(&job_id, &request, &control);
            } else {
                fail_acquisition_job_with_message(&job_id, message, "RAM edinimi basarisiz")
            }
        }
    }
}

/// Uzak agent üzerinde RAM edinimi başlatır ve çıkan dosyayı indirir.
fn run_remote_ram_job(job_id: String, request: RemoteRamRequest) {
    let target_path = match ram_output_path(
        &request.output,
        request.case_name.as_deref(),
        Some(&request.ip),
    ) {
        Ok(output) => output,
        Err(err) => {
            fail_acquisition_job_with_message(&job_id, err, "RAM edinimi basarisiz");
            return;
        }
    };
    let remote_file = ram_remote_file_name(&target_path.to_string_lossy());

    match RemoteConnection::connect(&request.ip, request.port, request.token.clone()) {
        Ok(mut connection) => {
            let remote_job_id = job_id.clone();
            match connection.start_remote_ram(&remote_file, Some(&remote_job_id), |done, total| {
                update_acquisition_progress_message(&job_id, done, total, "RAM edinimi sürüyor");
            }) {
                Ok(ram_result) => {
                    update_acquisition_message(&job_id, "RAM dosyası indiriliyor");
                    match connection.download_ram_file(
                        &remote_file,
                        &target_path,
                        Some(&remote_job_id),
                        |done, total| {
                            update_acquisition_progress_message(
                                &job_id,
                                done,
                                total,
                                "RAM dosyası indiriliyor",
                            );
                        },
                    ) {
                        Ok(download) => {
                            let remote_sha256 = download.sha256.clone().or(ram_result.sha256);
                            let sha256 = match finalize_ram_sha256(
                                &job_id,
                                &download.target_path,
                                remote_sha256,
                            ) {
                                Ok(value) => value,
                                Err(err) => {
                                    fail_acquisition_job_with_message(
                                        &job_id,
                                        err,
                                        "RAM hash olusturulamadi",
                                    );
                                    return;
                                }
                            };
                            finish_acquisition_job_with_message(
                                &job_id,
                                json!({
                                    "message": download.message,
                                    "remote_job_id": ram_result.job_id,
                                    "target_path": download.target_path,
                                    "bytes_transferred": download.bytes_transferred,
                                    "remote_bytes": ram_result.total_size,
                                    "sha256": sha256,
                                }),
                                "RAM edinimi tamamlandi",
                            );
                        }
                        Err(err) => fail_acquisition_job_with_message(
                            &job_id,
                            err.to_string(),
                            "RAM dosyası indirilemedi",
                        ),
                    }
                }
                Err(err) => fail_acquisition_job_with_message(
                    &job_id,
                    err.to_string(),
                    "RAM edinimi basarisiz",
                ),
            }
        }
        Err(err) => {
            fail_acquisition_job_with_message(&job_id, err.to_string(), "RAM edinimi basarisiz")
        }
    }
}

/// Tamamlanan RAM çıktısı için SHA-256 değerini sonuç veya dosya üzerinden kesinleştirir.
fn finalize_ram_sha256(
    job_id: &str,
    target_path: &Path,
    existing_sha256: Option<String>,
) -> Result<String, String> {
    update_acquisition_message(job_id, "RAM SHA256 olusturuluyor");
    let sha256 = match existing_sha256.filter(|value| !value.trim().is_empty()) {
        Some(value) => value,
        None => hash::calculate_file_hash(target_path, HashAlgorithm::Sha256)
            .map_err(|err| err.to_string())?,
    };
    hash::write_sha256_sidecar(target_path, &sha256).map_err(|err| err.to_string())?;
    Ok(sha256)
}

/// Seçilen RAM aracı için yerel root/admin gerekip gerekmediğini belirler.
fn local_ram_requires_elevation(tool: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        tool == "avml" && !process_is_root()
    }

    #[cfg(windows)]
    {
        tool == "winpmem" && !ram::is_root_or_admin()
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = tool;
        false
    }
}

/// RAM hatasının yetki yükseltmeyle tekrar denenebilir olup olmadığını belirler.
fn local_ram_error_can_retry_elevated(message: &str) -> bool {
    if !(cfg!(target_os = "linux") || cfg!(windows)) {
        return false;
    }
    let message = message.to_ascii_lowercase();
    message.contains("root")
        || message.contains("administrator")
        || message.contains("permission denied")
        || message.contains("access is denied")
        || message.contains("erişim engellendi")
        || message.contains("yetkisiz")
        || message.contains("os error 13")
}

/// Yerel RAM edinimini yetkili helper üzerinden çalıştırır.
fn run_elevated_local_ram_job(
    job_id: &str,
    request: &LocalRamRequest,
    control: &ram::CancellationToken,
) {
    update_acquisition_message(
        job_id,
        "Yetki bekleniyor: Linux'ta sudo/pkexec parola penceresini, Windows'ta UAC Evet/Hayır penceresini onaylayın.",
    );
    let stem = helper_file_stem("amele-ram-helper");
    let request_path = std::env::temp_dir().join(format!("{stem}-request.json"));
    let result_path = std::env::temp_dir().join(format!("{stem}-result.json"));
    let progress_path = std::env::temp_dir().join(format!("{stem}-progress.json"));
    let control_path = std::env::temp_dir().join(format!("{stem}-control.json"));

    let request_json = json!({
        "output_file": &request.output,
        "tool": request.tool.as_deref().unwrap_or_default(),
        "tool_path": &request.tool_path,
        "owner_uid": helper_owner_uid(),
        "owner_gid": helper_owner_gid(),
    });
    if let Err(err) = write_json_file(&request_path, &request_json) {
        fail_acquisition_job_with_message(job_id, err, "RAM edinimi basarisiz");
        return;
    }
    if let Err(err) = write_helper_control_state(&control_path, "running") {
        cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);
        fail_acquisition_job_with_message(job_id, err, "RAM edinimi basarisiz");
        return;
    }

    let args = vec![
        "ram-helper".to_string(),
        request_path.to_string_lossy().into_owned(),
        result_path.to_string_lossy().into_owned(),
        progress_path.to_string_lossy().into_owned(),
        control_path.to_string_lossy().into_owned(),
    ];
    let mut child = match spawn_elevated_helper(&args) {
        Ok(child) => child,
        Err(err) => {
            cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);
            fail_acquisition_job_with_message(job_id, err, "RAM edinimi basarisiz");
            return;
        }
    };
    update_acquisition_message(
        job_id,
        &format!("Yetki helper başlatıldı: {}", child.method()),
    );

    loop {
        if control.is_cancelled() {
            let _ = write_helper_control_state(&control_path, "cancelled");
            update_acquisition_message(job_id, "RAM edinimi iptal ediliyor");
            let mut exited = false;
            for _ in 0..30 {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        exited = true;
                        break;
                    }
                    Ok(None) => thread::sleep(std::time::Duration::from_millis(100)),
                    Err(_) => break,
                }
            }
            if !exited {
                let _ = child.kill();
                let _ = child.wait();
            }
            cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);
            fail_acquisition_job_with_message(
                job_id,
                "RAM edinimi iptal edildi".to_string(),
                "RAM edinimi basarisiz",
            );
            return;
        }
        if control.is_paused() {
            let _ = write_helper_control_state(&control_path, "paused");
            update_acquisition_message(job_id, "RAM edinimi duraklatildi");
        } else {
            let _ = write_helper_control_state(&control_path, "running");
        }

        if let Some((done, total, message)) = read_helper_progress(&progress_path) {
            update_acquisition_progress_message(job_id, done, total, &message);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let error = read_helper_error(&result_path)
                        .unwrap_or_else(|| child.failure_message(&status));
                    cleanup_helper_files(&[
                        &request_path,
                        &result_path,
                        &progress_path,
                        &control_path,
                    ]);
                    fail_acquisition_job_with_message(job_id, error, "RAM edinimi basarisiz");
                    return;
                }
                break;
            }
            Ok(None) => thread::sleep(std::time::Duration::from_millis(500)),
            Err(err) => {
                cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);
                fail_acquisition_job_with_message(job_id, err.to_string(), "RAM edinimi basarisiz");
                return;
            }
        }
    }

    let result = match read_helper_json(&result_path) {
        Ok(result) => result,
        Err(err) => {
            cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);
            fail_acquisition_job_with_message(job_id, err, "RAM edinimi basarisiz");
            return;
        }
    };
    cleanup_helper_files(&[&request_path, &result_path, &progress_path, &control_path]);

    if result.get("ok").and_then(Value::as_bool) == Some(true) {
        let Some(target_path) = result_target_path(&result) else {
            fail_acquisition_job_with_message(
                job_id,
                "RAM hedef dosyasi sonuc icinde bulunamadi".to_string(),
                "RAM hash olusturulamadi",
            );
            return;
        };
        let sha256 = match finalize_ram_sha256(job_id, &target_path, None) {
            Ok(value) => value,
            Err(err) => {
                fail_acquisition_job_with_message(job_id, err, "RAM hash olusturulamadi");
                return;
            }
        };
        finish_acquisition_job_with_message(
            job_id,
            json!({
                "message": "RAM edinimi tamamlandi",
                "target_path": result.get("target_path").cloned().unwrap_or(Value::Null),
                "bytes_written": result.get("bytes_written").cloned().unwrap_or(Value::Null),
                "sha256": sha256,
            }),
            "RAM edinimi tamamlandi",
        );
    } else {
        fail_acquisition_job_with_message(
            job_id,
            result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("RAM edinimi basarisiz")
                .to_string(),
            "RAM edinimi basarisiz",
        );
    }
}

/// RAM edinim sonucundaki hedef dosya yolunu JSON içinden çıkarır.
fn result_target_path(result: &Value) -> Option<PathBuf> {
    result
        .get("target_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

/// Vaka seçimine göre RAM çıktı dosyası yolunu hesaplar.
fn ram_output_path(
    output: &str,
    case_name: Option<&str>,
    remote_ip: Option<&str>,
) -> Result<PathBuf, String> {
    let vault = evidence_vault_for_output(case_name)?;
    let output = output.trim();
    let requested_file = ram_file_name_from_output(output);
    let seed_path = requested_file
        .map(|file_name| vault.ram_dir.join(file_name))
        .unwrap_or_else(|| vault.ram_dir.clone());

    Ok(canonical_ram_target_path(
        &seed_path.to_string_lossy(),
        remote_ip,
    ))
}

/// Kullanıcının verdiği output değerinden dosya adını çıkarır.
fn ram_file_name_from_output(output: &str) -> Option<&str> {
    let path = Path::new(output);
    let extension = path.extension()?.to_str()?;
    if !matches!(
        extension.to_ascii_lowercase().as_str(),
        "raw" | "mem" | "bin"
    ) {
        return None;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

/// Uzak RAM çıktısı için standart dosya adı üretir.
fn ram_remote_file_name(output: &str) -> String {
    Path::new(output)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "memory_dump.raw".to_string())
}

/// RAM çıktısı için IP ve tarih içeren standart hedef yol üretir.
fn canonical_ram_target_path(output: &str, remote_ip: Option<&str>) -> PathBuf {
    let output = PathBuf::from(output.trim());
    let is_file = output
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension, "raw" | "mem" | "bin"))
        .unwrap_or(false);
    let file_name = canonical_ram_file_name(
        remote_ip,
        is_file
            .then(|| output.file_name().and_then(|name| name.to_str()))
            .flatten(),
    );

    if is_file {
        output
            .parent()
            .map(|parent| parent.join(&file_name))
            .unwrap_or_else(|| PathBuf::from(file_name))
    } else {
        output.join(file_name)
    }
}

/// RAM dosyası için yerel/uzak durumuna göre standart dosya adı üretir.
fn canonical_ram_file_name(remote_ip: Option<&str>, current_name: Option<&str>) -> String {
    let remote_ip = remote_ip
        .map(sanitize_file_stem)
        .filter(|value| !value.is_empty());

    if let Some(name) = current_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(ip) = &remote_ip {
            let expected_prefix = format!("{ip}_ram_");
            if name.starts_with(&expected_prefix) && name.ends_with(".raw") {
                return name.to_string();
            }
            if name.starts_with("ram_") && name.ends_with(".raw") {
                return format!("{ip}_{name}");
            }
        } else if name.starts_with("ram_") && name.ends_with(".raw") {
            return name.to_string();
        }
    }

    let prefix = remote_ip
        .map(|ip| format!("{ip}_ram"))
        .unwrap_or_else(|| "ram".to_string());
    format!("{}_{}.raw", prefix, Local::now().format("%Y%m%d_%H%M%S"))
}

/// RAM dosyasında hızlı string/IOC taraması yapar.
pub fn ram_analyze_strings_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    match ram_analysis::analyze_ram_strings(path) {
        Ok(matches) => json_ok(serde_json::to_value(matches).unwrap_or(Value::Null)),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// RAM imajı için özet analiz endpoint'idir.
pub fn ram_analyze_summary_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };
    match ram_analysis::analyze_ram_summary_logged_with_symbol_dir(
        path,
        request.os_type.as_deref(),
        symbol_dir.as_deref(),
        None,
    ) {
        Ok(summary) => json_ok(serde_json::to_value(summary).unwrap_or(Value::Null)),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Volatility ön kontrolünü ve sembol ihtiyacını hesaplar.
pub fn ram_volatility_preflight_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let os_type = sanitize_ram_os_type(request.os_type.as_deref());
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };
    let preflight =
        crate::volatility::preflight_ram_image(path, &os_type, symbol_dir.as_deref(), None);
    json_ok(serde_json::to_value(preflight).unwrap_or(Value::Null))
}

/// Linux kernel sembol dosyasını indirip seçilen sembol klasörüne kurar.
pub fn ram_volatility_symbol_install_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }

    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }

    let os_type = sanitize_ram_os_type(request.os_type.as_deref());
    if os_type != "linux" {
        return json_ok(json!({
            "status": "windows-automatic",
            "installed": false,
            "message": "Windows sembolleri Volatility3 tarafından Microsoft symbol cache üzerinden otomatik yönetilir.",
            "symbol_dir": Value::Null,
            "banners": [],
            "matches": [],
        }));
    }

    let symbol_root = match writable_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(path) => path,
        Err(err) => return json_error(500, err),
    };
    let linux_symbol_dir = symbol_root.join("linux");
    if let Err(err) = fs::create_dir_all(&linux_symbol_dir) {
        return json_error(
            500,
            format!(
                "Volatility Linux symbol dizini oluşturulamadı: {} - {err}",
                linux_symbol_dir.display()
            ),
        );
    }

    let banners = match crate::volatility::scan_linux_banners(path, 3, None) {
        Ok(items) => items,
        Err(err) => {
            return json_error(
                500,
                format!("Linux kernel banner taraması başarısız: {err}"),
            );
        }
    };
    if banners.is_empty() {
        return json_error(
            404,
            "RAM imajında Linux kernel banner adayı bulunamadı. Dosyanın ham fiziksel RAM imajı olduğundan ve edinimin temiz tamamlandığından emin olun.",
        );
    }

    let mapping = match download_linux_symbol_mapping() {
        Ok(mapping) => mapping,
        Err(err) => {
            return json_error(
                500,
                format!(
                    "Linux symbol eşleme verisi indirilemedi: {err}. Kaynak: {VOLATILITY_LINUX_BANNERS_URL}"
                ),
            );
        }
    };

    let mut found = Vec::new();
    for banner in &banners {
        if let Some(paths) = mapping.get(banner) {
            for path in paths {
                found.push(json!({
                    "banner": banner,
                    "remote_path": path,
                    "url": linux_symbol_url(path),
                }));
            }
        }
    }

    if found.is_empty() {
        return json_ok(json!({
            "status": "not-found",
            "installed": false,
            "message": "Kernel banner bulundu ancak hazır remote ISF sembol veritabanında birebir eşleşme yok.",
            "source": VOLATILITY_LINUX_BANNERS_URL,
            "symbol_dir": symbol_root,
            "banners": banners,
            "matches": [],
            "recommendations": [
                "Kernel banner birebir eşleşmelidir; sadece sürüm numarası yeterli değildir.",
                "Bu kernel için debug vmlinux/System.map bulunup dwarf2json ile ISF üretilebilir.",
                "Üretilen .json veya .json.xz dosyasını symbol dizini altındaki linux klasörüne koyun."
            ],
        }));
    }

    let selected_path = found
        .first()
        .and_then(|item| item.get("remote_path"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let file_name = Path::new(&selected_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("linux-symbol.json.xz");
    let target = linux_symbol_dir.join(file_name);
    let mut downloaded = false;
    if !target.exists() {
        let url = linux_symbol_url(&selected_path);
        let download_target = target.with_extension("download");
        if let Err(err) = download_file_to_path(
            &url,
            &download_target,
            "Volatility Linux symbol download failed",
        ) {
            let _ = fs::remove_file(&download_target);
            return json_error(
                500,
                format!("Volatility Linux symbol dosyası indirilemedi: {err}. URL: {url}"),
            );
        }
        if let Err(err) = fs::rename(&download_target, &target) {
            let _ = fs::remove_file(&download_target);
            return json_error(
                500,
                format!(
                    "Volatility Linux symbol dosyası taşınamadı: {} - {err}",
                    target.display()
                ),
            );
        }
        downloaded = true;
    }

    let sha256 = sha256_file(&target).ok();
    let preflight = crate::volatility::preflight_ram_image(path, "linux", Some(&symbol_root), None);

    json_ok(json!({
        "status": if preflight.ready { "ready" } else { "installed" },
        "installed": true,
        "downloaded": downloaded,
        "message": if downloaded {
            "Linux Volatility3 symbol dosyası indirildi."
        } else {
            "Linux Volatility3 symbol dosyası zaten mevcut."
        },
        "source": VOLATILITY_LINUX_BANNERS_URL,
        "symbol_dir": symbol_root,
        "linux_symbol_dir": linux_symbol_dir,
        "target": target,
        "sha256": sha256,
        "banners": banners,
        "matches": found,
        "preflight": preflight,
    }))
}

/// RAM özet analizini uzun sürebileceği için arka plan işi olarak başlatır.
pub fn ram_analyze_summary_start_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };

    let os_type = sanitize_ram_os_type(request.os_type.as_deref());
    let (job_id, _control) = create_acquisition_job("RAM analizi başlatıldı");
    let thread_job_id = job_id.clone();
    thread::spawn(move || {
        run_ram_summary_analysis_job(thread_job_id, path, os_type, symbol_dir);
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
        "message": "RAM analizi başlatıldı",
    }))
}

/// RAM özet analiz arka plan işini çalıştırır.
fn run_ram_summary_analysis_job(
    job_id: String,
    path: PathBuf,
    os_type: String,
    symbol_dir: Option<PathBuf>,
) {
    update_acquisition_message(&job_id, "RAM analiz hazırlığı yapılıyor");
    let log_job_id = job_id.clone();
    let logger: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |line| {
        append_acquisition_log(&log_job_id, &line);
    });

    match ram_analysis::analyze_ram_summary_logged_with_symbol_dir(
        &path,
        Some(&os_type),
        symbol_dir.as_deref(),
        Some(logger),
    ) {
        Ok(summary) => finish_acquisition_job_with_message(
            &job_id,
            serde_json::to_value(summary).unwrap_or(Value::Null),
            "RAM analizi tamamlandı",
        ),
        Err(err) => {
            fail_acquisition_job_with_message(&job_id, err.to_string(), "RAM analizi başarısız")
        }
    }
}

/// RAM imajından basit dosya carving çıktıları üretir.
pub fn ram_carve_files_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let vault = match current_evidence_vault() {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match ram_analysis::carve_files(path, &vault.ram_dir) {
        Ok(carved) => json_ok(serde_json::to_value(carved).unwrap_or(Value::Null)),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Volatility ile RAM imajındaki prosesleri listeler.
pub fn ram_list_processes_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }

    let os = request.os_type.as_deref().unwrap_or("windows");
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };
    match crate::volatility::get_processes_logged_with_symbol_dir(
        path,
        os,
        symbol_dir.as_deref(),
        None,
    ) {
        Ok(procs) => {
            let mapped: Vec<Value> = procs
                .into_iter()
                .map(|p| {
                    json!({
                        "pid": p.pid.to_string(),
                        "name": format!("{} ({})", p.name, p.offset),
                        "dump_size": 0,
                    })
                })
                .collect();
            json_ok(Value::Array(mapped))
        }
        Err(err) => json_error(500, err),
    }
}

/// Proses listelemeyi arka plan işi olarak başlatır.
pub fn ram_list_processes_start_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };

    let os_type = sanitize_ram_os_type(request.os_type.as_deref());
    let (job_id, _control) = create_acquisition_job("RAM proses analizi başlatıldı");
    let thread_job_id = job_id.clone();
    thread::spawn(move || {
        run_ram_process_list_job(thread_job_id, path, os_type, symbol_dir);
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
        "message": "RAM proses analizi başlatıldı",
    }))
}

/// Proses listeleme arka plan işini çalıştırır.
fn run_ram_process_list_job(
    job_id: String,
    path: PathBuf,
    os_type: String,
    symbol_dir: Option<PathBuf>,
) {
    update_acquisition_message(&job_id, "Volatility3 proses listesi çıkarılıyor");
    let log_job_id = job_id.clone();
    let logger: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |line| {
        append_acquisition_log(&log_job_id, &line);
    });

    match crate::volatility::get_processes_logged_with_symbol_dir(
        &path,
        &os_type,
        symbol_dir.as_deref(),
        Some(logger),
    ) {
        Ok(procs) => {
            let mapped: Vec<Value> = procs
                .into_iter()
                .map(|p| {
                    json!({
                        "pid": p.pid.to_string(),
                        "name": format!("{} ({})", p.name, p.offset),
                        "dump_size": 0,
                        "extra_info": p.extra_info,
                    })
                })
                .collect();
            finish_acquisition_job_with_message(
                &job_id,
                Value::Array(mapped),
                "RAM proses listesi hazır",
            );
        }
        Err(err) => fail_acquisition_job_with_message(&job_id, err, "RAM proses analizi başarısız"),
    }
}

/// UI'den gelen OS tipini desteklenen windows/linux değerine indirger.
fn sanitize_ram_os_type(value: Option<&str>) -> String {
    match value {
        Some("linux") => "linux".to_string(),
        _ => "windows".to_string(),
    }
}

/// İstekten gelen sembol klasörünü doğrular.
fn request_symbol_dir(value: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(raw);
    if !path.exists() {
        return Err(format!("Volatility symbol dizini bulunamadı: {raw}"));
    }
    if !path.is_dir() {
        return Err(format!("Volatility symbol yolu klasör değil: {raw}"));
    }
    Ok(Some(path))
}

/// Sembol kurulumu için yazılabilir klasör seçer.
fn writable_symbol_dir(value: Option<&str>) -> Result<PathBuf, String> {
    let path = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| *value != ".symbols")
        .map(PathBuf::from)
        .unwrap_or_else(default_amele_symbol_dir);
    fs::create_dir_all(&path).map_err(|err| {
        format!(
            "Volatility symbol dizini oluşturulamadı: {} - {err}",
            path.display()
        )
    })?;
    if !path.is_dir() {
        return Err(format!(
            "Volatility symbol yolu klasör değil: {}",
            path.display()
        ));
    }
    Ok(path)
}

/// Amele varsayılan Volatility sembol klasörünü döndürür.
fn default_amele_symbol_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Amele")
        .join(".symbols")
}

/// Linux kernel banner ve sembol URL eşlemesini indirir.
fn download_linux_symbol_mapping() -> Result<BTreeMap<String, Vec<String>>, String> {
    let temp_dir = std::env::temp_dir().join("amele-volatility-symbols");
    fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    let mapping_path = temp_dir.join("banners_plain.json");
    download_file_to_path(
        VOLATILITY_LINUX_BANNERS_URL,
        &mapping_path,
        "Volatility Linux symbol mapping download failed",
    )?;
    let content = fs::read_to_string(&mapping_path).map_err(|err| err.to_string())?;
    let _ = fs::remove_file(&mapping_path);
    serde_json::from_str::<BTreeMap<String, Vec<String>>>(&content)
        .map_err(|err| format!("Volatility Linux symbol eşleme JSON'u okunamadı: {err}"))
}

/// Release içindeki sembol dosyası yolunu tam indirme URL'sine çevirir.
fn linux_symbol_url(remote_path: &str) -> String {
    format!(
        "{}{}",
        VOLATILITY_LINUX_SYMBOL_RAW_BASE,
        remote_path.trim_start_matches('/').replace(' ', "%20")
    )
}

/// Seçilen proses için DLL/açık dosya gibi ayrıntıları döndürür.
pub fn ram_process_details_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        pid: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }

    let os = request.os_type.as_deref().unwrap_or("windows");
    let pid_num = match request.pid.parse::<i64>() {
        Ok(n) => n,
        Err(_) => return json_error(400, "PID must be a valid integer for Volatility3"),
    };
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };
    match crate::volatility::get_process_details_logged_with_symbol_dir(
        path,
        os,
        pid_num,
        symbol_dir.as_deref(),
        None,
    ) {
        Ok(details) => json_ok(json!({
            "maps": details,
            "dumps": Vec::<String>::new(),
        })),
        Err(err) => json_error(500, err),
    }
}

/// Proses ayrıntı analizini arka plan işi olarak başlatır.
pub fn ram_process_details_start_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
        pid: String,
        os_type: Option<String>,
        symbol_dir: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    let path = PathBuf::from(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }
    let pid_num = match request.pid.parse::<i64>() {
        Ok(n) => n,
        Err(_) => return json_error(400, "PID must be a valid integer for Volatility3"),
    };
    let symbol_dir = match request_symbol_dir(request.symbol_dir.as_deref()) {
        Ok(dir) => dir,
        Err(err) => return json_error(404, err),
    };

    let os_type = sanitize_ram_os_type(request.os_type.as_deref());
    let (job_id, _control) = create_acquisition_job("RAM proses detayı başlatıldı");
    let thread_job_id = job_id.clone();
    thread::spawn(move || {
        run_ram_process_details_job(thread_job_id, path, os_type, pid_num, symbol_dir);
    });

    json_ok(json!({
        "job_id": job_id,
        "status": "running",
        "message": "RAM proses detayı başlatıldı",
    }))
}

/// Proses ayrıntı arka plan işini çalıştırır.
fn run_ram_process_details_job(
    job_id: String,
    path: PathBuf,
    os_type: String,
    pid: i64,
    symbol_dir: Option<PathBuf>,
) {
    update_acquisition_message(&job_id, "Volatility3 proses detayı çıkarılıyor");
    let log_job_id = job_id.clone();
    let logger: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |line| {
        append_acquisition_log(&log_job_id, &line);
    });

    match crate::volatility::get_process_details_logged_with_symbol_dir(
        &path,
        &os_type,
        pid,
        symbol_dir.as_deref(),
        Some(logger),
    ) {
        Ok(details) => finish_acquisition_job_with_message(
            &job_id,
            json!({
                "maps": details,
                "dumps": Vec::<String>::new(),
            }),
            "RAM proses detayı hazır",
        ),
        Err(err) => fail_acquisition_job_with_message(&job_id, err, "RAM proses detayı başarısız"),
    }
}

/// RAM imajı içinde kullanıcı sorgusuna göre ham arama yapar.
pub fn ram_process_search_endpoint(body: &[u8]) -> Response {
    #[allow(dead_code)]
    #[derive(Deserialize)]
    struct Request {
        path: String,
        pid: String,
        query: String,
        os_type: Option<String>,
    }
    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };
    if request.query.trim().is_empty() {
        return json_error(400, "Arama sorgusu gerekli / Search query required");
    }
    let path = Path::new(&request.path);
    if !path.exists() {
        return json_error(404, "Bellek dosyası bulunamadı / Memory file not found");
    }

    match ram_analysis::search_raw_memory(path, &request.query) {
        Ok(matches) => json_ok(serde_json::to_value(matches).unwrap_or(Value::Null)),
        Err(err) => json_error(500, err.to_string()),
    }
}

/// Carving ile çıkarılmış dosyanın güvenli ön izlemesini döndürür.
pub fn ram_read_carved_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct Request {
        path: String,
    }

    let request: Request = match serde_json::from_slice(body) {
        Ok(req) => req,
        Err(err) => return json_error(400, err.to_string()),
    };

    let target_path = PathBuf::from(request.path.trim());
    if !target_path.exists() {
        return json_error(404, "Dosya bulunamadı / File not found");
    }

    let vault = match current_evidence_vault() {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !target_path.starts_with(&vault.ram_dir) {
        return json_error(403, "Yetkisiz erişim / Access denied");
    }

    let ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let size = match fs::metadata(&target_path) {
        Ok(meta) => meta.len(),
        Err(err) => return json_error(500, err.to_string()),
    };

    if ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&ext.as_str()) {
        match fs::read(&target_path) {
            Ok(bytes) => {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let mime = match ext.as_str() {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    _ => "image/png",
                };
                return json_ok(json!({
                    "type": "image",
                    "mime": mime,
                    "content": format!("data:{};base64,{}", mime, encoded),
                    "size": size,
                }));
            }
            Err(err) => return json_error(500, err.to_string()),
        }
    }

    let is_text_ext =
        ["txt", "log", "json", "xml", "plist"].contains(&ext.as_str()) || size < 100_000;

    match fs::File::open(&target_path) {
        Ok(mut f) => {
            let mut buf = vec![0_u8; 16384.min(size as usize)];
            let read = f.read(&mut buf).unwrap_or(0);
            let content_bytes = &buf[..read];

            if is_text_ext {
                if let Ok(text) = std::str::from_utf8(content_bytes) {
                    return json_ok(json!({
                        "type": "text",
                        "content": text,
                        "size": size,
                        "truncated": size > 16384,
                    }));
                }
            }

            let mut hex_lines = Vec::new();
            for chunk in content_bytes.chunks(16) {
                let offset = (hex_lines.len() * 16) as u64;
                let hex_parts: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
                let hex_str = hex_parts.join(" ");
                let ascii_str: String = chunk
                    .iter()
                    .map(|&b| {
                        if b.is_ascii_graphic() || b == b' ' {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                hex_lines.push(format!("{:08X}  {:48}  |{}|", offset, hex_str, ascii_str));
            }
            json_ok(json!({
                "type": "hex",
                "content": hex_lines.join("\n"),
                "size": size,
                "truncated": size > 16384,
            }))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}
