//! Yerel HTTP sunucusunu ve native pencere açılışını yönetir.
use crate::router;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

const DEV_UI_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ui");

/// HTTP cevabının durum kodu, içerik tipi ve gövdesini taşır.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    /// Gövdesiz HTTP cevabı üretir.
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: Vec::new(),
        }
    }
}

/// Başarılı JSON API cevabı üretir.
pub fn json_ok(value: serde_json::Value) -> Response {
    Response {
        status: 200,
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
    }
}

/// Hata mesajını sınıflandırıp ayrıntılı JSON hata cevabına dönüştürür.
pub fn json_error(status: u16, message: impl Into<String>) -> Response {
    let message = message.into();
    let advice = crate::diagnostics::classify_error(&message);
    Response {
        status,
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&serde_json::json!({
            "ok": false,
            "error": message,
            "code": advice.code,
            "detail": advice.detail,
            "suggestion": advice.suggestion,
        }))
        .unwrap_or_else(|_| b"{\"ok\":false}".to_vec()),
    }
}

/// Yerel backend'i başlatır ve native WebView penceresini açar.
pub fn run_native() -> Result<(), String> {
    crate::native_window::prepare_environment()?;
    let url = start_background()?;
    let native_url = format!("{url}?native=1");
    println!("Amele native UI: {native_url}");
    crate::native_window::run(&native_url)
}

/// Yerel backend'i başlatır ve debug için sistem tarayıcısını açar.
pub fn run_browser() -> Result<(), String> {
    let url = start_background()?;
    println!("Amele UI backend: {url}");
    open_window(&url);
    loop {
        thread::park();
    }
}

/// Rastgele boş localhost portunda UI backend thread'ini başlatır.
fn start_background() -> Result<String, String> {
    validate_ui_assets()?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|err| {
        crate::diagnostics::startup_error("Yerel UI backend portu acilamadi.", &err.to_string())
    })?;
    let addr = listener.local_addr().map_err(|err| {
        crate::diagnostics::startup_error("Yerel UI backend adresi okunamadi.", &err.to_string())
    })?;
    let url = format!("http://{addr}/");

    // Portu API state'ine kaydet (developer konsol penceresi için)
    crate::api::set_server_port(addr.port());

    // Başlangıç logları
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "server:startup",
        format!(
            "Amele {} baslatildi | OS: {} {} {} | PID: {} | EXE: {:?} | UI: {:?} | PORT: {}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::FAMILY,
            std::env::consts::ARCH,
            std::process::id(),
            std::env::current_exe().unwrap_or_default(),
            ui_root(),
            addr.port(),
        ),
    );
    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "server:startup",
        format!(
            "CWD: {:?} | HOME: {:?} | APPDIR: {:?} | DISPLAY: {:?} | WAYLAND: {:?}",
            std::env::current_dir().unwrap_or_default(),
            std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
            std::env::var_os("APPDIR"),
            std::env::var_os("DISPLAY"),
            std::env::var_os("WAYLAND_DISPLAY"),
        ),
    );

    #[cfg(target_os = "linux")]
    {
        // Linux'ta yetki ve ortam bilgisi
        let is_root = crate::api::process_is_root();
        crate::logging::runtime_log(
            crate::logging::LogLevel::Info,
            "server:startup",
            format!(
                "Linux is_root: {} | XDG_DESKTOP: {:?} | GDK_BACKEND: {:?} | WEBKIT_EXEC: {:?}",
                is_root,
                std::env::var_os("XDG_CURRENT_DESKTOP"),
                std::env::var_os("GDK_BACKEND"),
                std::env::var_os("WEBKIT_EXEC_PATH"),
            ),
        );
    }

    thread::Builder::new()
        .name("amele-ui-server".to_string())
        .spawn(move || serve(listener))
        .map_err(|err| {
            crate::diagnostics::startup_error(
                "Yerel UI backend thread'i baslatilamadi.",
                &err.to_string(),
            )
        })?;

    crate::logging::runtime_log(
        crate::logging::LogLevel::Info,
        "server:startup",
        format!("UI sunucusu hazir: {url}"),
    );

    Ok(url)
}

/// UI dosyalarının paket içinde veya geliştirme klasöründe bulunduğunu doğrular.
fn validate_ui_assets() -> Result<(), String> {
    let root = ui_root();
    if root.join("index.html").is_file() {
        Ok(())
    } else {
        Err(crate::diagnostics::ui_assets_missing(&root))
    }
}

/// TCP listener üzerinden gelen istekleri ayrı thread'lerde karşılar.
fn serve(listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| {
                    if let Err(err) = handle_stream(stream) {
                        crate::logging::runtime_log(
                            crate::logging::LogLevel::Warn,
                            "server:http",
                            format!("HTTP akisi isleme hatasi: {err}"),
                        );
                        eprintln!("UI request failed: {err}");
                    }
                });
            }
            Err(err) => {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Error,
                    "server:http",
                    format!("TCP baglanti hatasi: {err}"),
                );
                eprintln!("UI connection failed: {err}");
            }
        }
    }
}

/// Debug/browser modunda uygun tarayıcı komutunu seçip pencere açar.
fn open_window(url: &str) {
    let browsers: &[(&str, &[&str])] = &[
        (
            "chromium",
            &["--new-window", "--no-first-run", "--app", url],
        ),
        (
            "google-chrome",
            &["--new-window", "--no-first-run", "--app", url],
        ),
        (
            "google-chrome-stable",
            &["--new-window", "--no-first-run", "--app", url],
        ),
        (
            "brave-browser",
            &["--new-window", "--no-first-run", "--app", url],
        ),
        ("firefox", &[url]),
        ("xdg-open", &[url]),
    ];

    for (program, args) in browsers {
        if Command::new(program)
            .args(*args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
    }

    eprintln!("Browser could not be opened automatically. Use: {url}");
}

/// Ham TCP stream'den HTTP isteğini okur, router'a verir ve cevabı yazar.
fn handle_stream(stream: TcpStream) -> Result<(), String> {
    let peer = stream.peer_addr().ok();
    if peer.map(|addr| !addr.ip().is_loopback()).unwrap_or(true) {
        return Err("non-loopback request rejected".to_string());
    }

    let mut reader = BufReader::new(stream.try_clone().map_err(|err| err.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|err| err.to_string())?;
    if request_line.trim().is_empty() {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();
    let mut content_length = 0_usize;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|err| err.to_string())?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }

    let mut body = vec![0_u8; content_length];
    if content_length > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|err| err.to_string())?;
    }

    let t0 = std::time::Instant::now();
    let response = match std::panic::catch_unwind(|| {
        router::route_request(&method, &raw_path, &body)
    }) {
        Ok(response) => response,
        Err(payload) => {
            let panic = crate::diagnostics::panic_payload(payload.as_ref());
            crate::logging::runtime_log(
                crate::logging::LogLevel::Error,
                "server:panic",
                format!("PANIK: {method} {raw_path} — {panic}"),
            );
            eprintln!("UI API panic: {method} {raw_path}: {panic}");
            json_error(
                500,
                format!(
                    "Backend istegi islenirken beklenmeyen hata olustu: {method} {raw_path}: {panic}"
                ),
            )
        }
    };

    // API isteklerini (statik dosya değil) logla
    if raw_path.starts_with("/api/") {
        let elapsed = t0.elapsed();
        let status = response.status;
        let level = if status >= 500 {
            crate::logging::LogLevel::Error
        } else if status >= 400 {
            crate::logging::LogLevel::Warn
        } else {
            crate::logging::LogLevel::Debug
        };
        crate::logging::runtime_log(
            level,
            "server:api",
            format!(
                "{} {} → {} [{:.1}ms]",
                method,
                raw_path,
                status,
                elapsed.as_secs_f64() * 1000.0,
            ),
        );
        // Hatalı yanıtların gövdesini de logla (ilk 400 byte)
        if status >= 400 {
            let body_preview = String::from_utf8_lossy(&response.body);
            let preview = body_preview.trim();
            let short = if preview.len() > 400 {
                &preview[..400]
            } else {
                preview
            };
            crate::logging::runtime_log(
                level,
                "server:api:error",
                format!("{method} {raw_path} hata govdesi: {short}"),
            );
        }
    }

    write_response(stream, response)
}

/// HTTP header ve gövdesini TCP stream'e yazar.
fn write_response(mut stream: TcpStream, response: Response) -> Result<(), String> {
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        499 => "Client Closed Request",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: http://127.0.0.1\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .map_err(|err| err.to_string())
}

/// UI dosyalarının paketlenmiş veya geliştirme ortamındaki kök klasörünü bulur.
pub fn ui_root() -> PathBuf {
    if let Some(path) = std::env::var_os("AMELE_UI_ROOT") {
        let path = PathBuf::from(path);
        if path.join("index.html").exists() {
            return path;
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(bin_dir) = exe.parent()
        && let Some(prefix) = bin_dir.parent()
    {
        let packaged = prefix.join("share").join("amele").join("ui");
        if packaged.join("index.html").exists() {
            return packaged;
        }
    }

    PathBuf::from(DEV_UI_ROOT)
}

/// Dosya uzantısına göre HTTP Content-Type döndürür.
pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// URL path içindeki yüzde kodlamasını UTF-8 stringe çevirir.
pub fn percent_decode(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]).ok_or(())?;
                let low = hex_value(bytes[index + 2]).ok_or(())?;
                out.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

/// Tek hex karakterini sayısal nibble değerine çevirir.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
// o gözlerin geçmeyen hisleri var
