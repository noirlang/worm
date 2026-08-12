//! Docker ve konteyner ortamları için adli bilişim (Container DFIR) analiz ve edinim modülüdür.
//!
//! Bu modül, hem çalışan (canlı) Linux sistemlerindeki Docker daemon ve `/var/lib/docker/` yapısını,
//! hem de disk imajı olarak bağlanmış (soğuk/offline) `/var/lib/docker/` dizinlerini ayrıştırabilir.
//!
//! Temel Yetenekler:
//! - `config.v2.json` ve `hostconfig.json` ayrıştırma (konteyner meta verileri, mount'lar, ağlar).
//! - Ortam değişkenlerinde (`ENV`) parola, token ve API anahtarı (secret) tespiti.
//! - Konteynerden kaçış (Container Escape) risk analizi (`--privileged`, `/var/run/docker.sock` mount, `hostPID` vb.).
//! - `UpperDir` (OverlayFS diff/drift) katmanının tespiti ve delil olarak `.tar.gz` arşivlenmesi.
//! - Konteyner JSON loglarının (`<id>-json.log`) okunması ve zaman çizelgesine dökülmesi.
//! - Vaka klasörüne SHA-256 hash doğrulamalı adli paket üretimi.

use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::evidence::EvidenceVault;
use crate::hash::{HashAlgorithm, calculate_file_hash};
use chrono::Local;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tar::Builder;

/// Varsayılan Docker kök dizini yolu
pub const DEFAULT_DOCKER_ROOT: &str = "/var/lib/docker";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Konteyner güvenlik risk seviyesini belirtir.
pub enum RiskLevel {
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "CRITICAL")]
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "LOW",
            RiskLevel::Medium => "MEDIUM",
            RiskLevel::High => "HIGH",
            RiskLevel::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Konteynerin ortam değişkeninde tespit edilen hassas veriyi taşır.
pub struct DetectedSecret {
    pub key: String,
    pub value_preview: String,
    pub secret_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Konteyner mount eşleşmesini taşır.
pub struct ContainerMount {
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
    pub propagation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Konteynerin özet adli bilişim bilgilerini taşır.
pub struct DockerContainerSummary {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub created: String,
    pub state: String,
    pub running: bool,
    pub pid: u32,
    pub exit_code: i32,
    pub upper_dir: Option<String>,
    pub merged_dir: Option<String>,
    pub work_dir: Option<String>,
    pub log_path: Option<String>,
    pub ip_address: Option<String>,
    pub ports: Vec<String>,
    pub privileged: bool,
    pub risk_level: RiskLevel,
    pub risk_reasons: Vec<String>,
    pub mounts: Vec<ContainerMount>,
    pub secrets_found: Vec<DetectedSecret>,
    pub driver: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Docker sistem durumunu temsil eder.
pub struct DockerSystemStatus {
    pub docker_available: bool,
    pub docker_running: bool,
    pub storage_driver: String,
    pub root_dir: String,
    pub containers_count: usize,
    pub running_count: usize,
    pub paused_count: usize,
    pub stopped_count: usize,
    pub is_offline: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Docker adli edinim talebini taşır.
pub struct DockerAcquisitionRequest {
    pub container_id: String,
    pub acquire_diff: bool,
    pub acquire_logs: bool,
    pub acquire_config: bool,
    pub case_name: Option<String>,
    pub custom_docker_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Docker edinim işleminin sonucunu taşır.
pub struct DockerAcquisitionResult {
    pub job_id: String,
    pub container_id: String,
    pub container_name: String,
    pub case_path: String,
    pub diff_tar_path: Option<String>,
    pub diff_sha256: Option<String>,
    pub diff_size_bytes: u64,
    pub config_saved: bool,
    pub logs_saved: bool,
    pub metadata_saved: bool,
    pub files_acquired: Vec<String>,
    pub message: String,
}

/// Docker servis durumunu veya hedef dizindeki Docker varlığını kontrol eder.
pub fn check_docker_status(custom_root: Option<&Path>) -> DockerSystemStatus {
    let root_path = custom_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DOCKER_ROOT));

    let is_custom = custom_root.is_some();
    let containers_dir = root_path.join("containers");
    let overlay2_dir = root_path.join("overlay2");

    let containers_exist = containers_dir.exists() && containers_dir.is_dir();
    let overlay_exist = overlay2_dir.exists() && overlay2_dir.is_dir();

    let is_available = containers_exist || overlay_exist;

    // Canlı sistemde socket kontrolü
    let socket_running = if !is_custom {
        Path::new("/var/run/docker.sock").exists()
    } else {
        false
    };

    let mut containers_count = 0;
    let mut running_count = 0;
    let mut paused_count = 0;
    let mut stopped_count = 0;
    let mut permission_denied = false;

    if containers_exist {
        match fs::read_dir(&containers_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        containers_count += 1;
                        let config_file = entry.path().join("config.v2.json");
                        if let Ok(content) = fs::read_to_string(config_file) {
                            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                                let running = val
                                    .get("State")
                                    .and_then(|s| s.get("Running"))
                                    .and_then(|r| r.as_bool())
                                    .unwrap_or(false);
                                let paused = val
                                    .get("State")
                                    .and_then(|s| s.get("Paused"))
                                    .and_then(|p| p.as_bool())
                                    .unwrap_or(false);
                                if running {
                                    running_count += 1;
                                } else if paused {
                                    paused_count += 1;
                                } else {
                                    stopped_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                permission_denied = true;
            }
            _ => {}
        }
    }

    let storage_driver = if overlay_exist {
        "overlay2".to_string()
    } else if root_path.join("vfs").exists() {
        "vfs".to_string()
    } else if root_path.join("btrfs").exists() {
        "btrfs".to_string()
    } else if root_path.join("zfs").exists() {
        "zfs".to_string()
    } else {
        "unknown".to_string()
    };

    let message = if permission_denied {
        "Docker dizinine (/var/lib/docker) erişim izni yok. Root / sudo yetkisi gerekebilir."
            .to_string()
    } else if is_custom {
        format!(
            "Bağlanmış disk imajından Docker dizini tarandı ({})",
            root_path.display()
        )
    } else if socket_running {
        "Docker servisi çalışıyor ve adli analize hazır.".to_string()
    } else if is_available {
        "Docker dizini bulundu (servis durdurulmuş veya pasif).".to_string()
    } else {
        "Docker dizini (/var/lib/docker) bulunamadı.".to_string()
    };

    DockerSystemStatus {
        docker_available: is_available,
        docker_running: socket_running,
        storage_driver,
        root_dir: root_path.to_string_lossy().to_string(),
        containers_count,
        running_count,
        paused_count,
        stopped_count,
        is_offline: is_custom,
        message,
    }
}

/// Ortam değişkenlerinde parola, gizli anahtar veya token kalıplarını tarar.
pub fn scan_env_for_secrets(env_list: &[String]) -> Vec<DetectedSecret> {
    let mut secrets = Vec::new();
    let secret_keywords = [
        ("PASSWORD", "Password / Credential"),
        ("PASSWD", "Password / Credential"),
        ("SECRET", "Secret Key / Salt"),
        ("TOKEN", "Access / Bearer Token"),
        ("API_KEY", "API Key"),
        ("APIKEY", "API Key"),
        ("PRIVATE_KEY", "Private Key"),
        ("ACCESS_KEY", "Cloud Access Key"),
        ("AUTH", "Auth Credentials"),
        ("JWT", "JSON Web Token"),
        ("DB_PASS", "Database Password"),
        ("MYSQL_ROOT_PASSWORD", "MySQL Root Password"),
        ("POSTGRES_PASSWORD", "Postgres Password"),
        ("REDIS_PASSWORD", "Redis Password"),
    ];

    for item in env_list {
        if let Some((key, val)) = item.split_once('=') {
            let upper_key = key.to_uppercase();
            for (kw, kind) in &secret_keywords {
                if upper_key.contains(kw) && !val.trim().is_empty() {
                    let preview = if val.len() <= 6 {
                        "******".to_string()
                    } else {
                        format!("{}...{}", &val[..2], &val[val.len() - 2..])
                    };
                    secrets.push(DetectedSecret {
                        key: key.to_string(),
                        value_preview: preview,
                        secret_type: kind.to_string(),
                    });
                    break;
                }
            }
        }
    }
    secrets
}

/// Verilen konteyner konfigürasyonunu analiz edip güvenlik risklerini belirler.
pub fn evaluate_container_risk(
    config_v2: &Value,
    host_config: &Value,
    mounts: &[ContainerMount],
) -> (RiskLevel, Vec<String>) {
    let mut reasons = Vec::new();
    let mut score: u32 = 0;

    // 1. Privileged kontrolü
    let privileged = host_config
        .get("Privileged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if privileged {
        score += 50;
        reasons.push("Konteyner tam yetkili (--privileged) modda çalışıyor.".to_string());
    }

    // 2. Docker Socket mount kontrolü (En yaygın Container Escape yolu)
    let docker_sock_mounted = mounts
        .iter()
        .any(|m| m.source.contains("docker.sock") || m.destination.contains("docker.sock"));
    if docker_sock_mounted {
        score += 50;
        reasons.push(
            "Host Docker soketi (/var/run/docker.sock) konteyner içine mount edilmiş (DoD Escape riski)."
                .to_string(),
        );
    }

    // 3. Kritik host dizinlerinin mount edilmesi
    for mount in mounts {
        let s = mount.source.as_str();
        if s == "/" || s == "/etc" || s == "/root" || s == "/home" || s == "/proc" || s == "/sys" {
            score += 30;
            reasons.push(format!(
                "Kritik host dizini doğrudan bağlı: {} -> {}",
                mount.source, mount.destination
            ));
        }
    }

    // 4. Host namespace paylaşımı
    if let Some(net_mode) = host_config.get("NetworkMode").and_then(|v| v.as_str()) {
        if net_mode == "host" {
            score += 20;
            reasons.push("Host ağ ad alanı (NetworkMode: host) doğrudan kullanılıyor.".to_string());
        }
    }

    if let Some(pid_mode) = host_config.get("PidMode").and_then(|v| v.as_str()) {
        if pid_mode == "host" {
            score += 25;
            reasons.push("Host süreç ad alanı (PidMode: host) paylaşılıyor.".to_string());
        }
    }

    if let Some(ipc_mode) = host_config.get("IpcMode").and_then(|v| v.as_str()) {
        if ipc_mode == "host" {
            score += 15;
            reasons.push("Host IPC ad alanı (IpcMode: host) paylaşılıyor.".to_string());
        }
    }

    // 5. İlave tehlikeli yetkiler (Capabilities)
    if let Some(cap_add) = host_config.get("CapAdd").and_then(|v| v.as_array()) {
        for cap in cap_add {
            if let Some(c) = cap.as_str() {
                if c == "SYS_ADMIN" || c == "ALL" || c == "NET_ADMIN" || c == "SYS_PTRACE" {
                    score += 20;
                    reasons.push(format!("Tehlikeli Linux yetkisi eklenmiş: {}", c));
                }
            }
        }
    }

    // 6. Güvenlik profillerinin devre dışı olması
    if let Some(apparmor) = host_config.get("AppArmorProfile").and_then(|v| v.as_str()) {
        if apparmor == "unconfined" {
            score += 15;
            reasons.push("AppArmor koruması devre dışı bırakılmış (unconfined).".to_string());
        }
    }

    if let Some(sec_opts) = host_config.get("SecurityOpt").and_then(|v| v.as_array()) {
        for opt in sec_opts {
            if let Some(s) = opt.as_str() {
                if s.contains("seccomp=unconfined") {
                    score += 15;
                    reasons.push("Seccomp filtreleri devre dışı (unconfined).".to_string());
                }
            }
        }
    }

    // 7. Root kullanıcısı ile mi çalışıyor?
    let user = config_v2
        .get("Config")
        .and_then(|c| c.get("User"))
        .and_then(|u| u.as_str())
        .unwrap_or("");
    if user.is_empty() || user == "0" || user == "root" {
        score += 5;
    }

    let level = if score >= 50 {
        RiskLevel::Critical
    } else if score >= 30 {
        RiskLevel::High
    } else if score >= 15 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    (level, reasons)
}

/// Verilen Docker kök dizinindeki tüm konteynerleri listeler ve adli analizini yapar.
pub fn list_containers(custom_root: Option<&Path>) -> AmeleResult<Vec<DockerContainerSummary>> {
    let root_path = custom_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DOCKER_ROOT));

    let containers_dir = root_path.join("containers");
    if !containers_dir.exists() || !containers_dir.is_dir() {
        if custom_root.is_none() {
            if let Ok(cli_list) = list_containers_from_cli() {
                if !cli_list.is_empty() {
                    return Ok(cli_list);
                }
            }
        }
        return Ok(Vec::new());
    }

    let mut list = Vec::new();

    let entries = match fs::read_dir(&containers_dir) {
        Ok(e) => e,
        Err(e) => {
            if custom_root.is_none() {
                if let Ok(cli_list) = list_containers_from_cli() {
                    if !cli_list.is_empty() {
                        return Ok(cli_list);
                    }
                }
            }
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                return Err(AmeleError::new(
                    HataKodu::YetkisizErisim,
                    format!(
                        "Docker dizinine ({}) erişim reddedildi. Root/sudo yetkisi gerekebilir.",
                        containers_dir.display()
                    ),
                ));
            } else {
                return Err(AmeleError::io(
                    HataKodu::DosyaOkuma,
                    format!(
                        "Docker konteyner dizini okunamadı: {}",
                        containers_dir.display()
                    ),
                    e,
                ));
            }
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let container_id = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_default()
            .to_string();

        if container_id.len() < 12 {
            continue;
        }

        let config_path = path.join("config.v2.json");
        let hostconfig_path = path.join("hostconfig.json");

        let config_v2 = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        let host_config = if hostconfig_path.exists() {
            fs::read_to_string(&hostconfig_path)
                .ok()
                .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        let summary =
            parse_single_container(&config_v2, &host_config, &container_id, path.to_str());
        list.push(summary);
    }

    if list.is_empty() && custom_root.is_none() {
        if let Ok(cli_list) = list_containers_from_cli() {
            if !cli_list.is_empty() {
                return Ok(cli_list);
            }
        }
    }

    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(list)
}

/// Tek bir konteyner verisini (config_v2 ve host_config) adli bilişim özetine dönüştürür.
pub fn parse_single_container(
    config_v2: &Value,
    host_config: &Value,
    container_id: &str,
    container_path: Option<&str>,
) -> DockerContainerSummary {
    let effective_host_config =
        if host_config.is_object() && !host_config.as_object().unwrap().is_empty() {
            host_config
        } else {
            config_v2.get("HostConfig").unwrap_or(host_config)
        };

    let short_id = if container_id.len() >= 12 {
        container_id[..12].to_string()
    } else {
        container_id.to_string()
    };

    let name = config_v2
        .get("Name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();

    let image = config_v2
        .get("Config")
        .and_then(|c| c.get("Image"))
        .and_then(|i| i.as_str())
        .or_else(|| config_v2.get("Image").and_then(|i| i.as_str()))
        .unwrap_or("unknown")
        .to_string();

    let created = config_v2
        .get("Created")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let state_val = config_v2.get("State").cloned().unwrap_or(Value::Null);
    let running = state_val
        .get("Running")
        .and_then(|r| r.as_bool())
        .unwrap_or(false);
    let pid = state_val.get("Pid").and_then(|p| p.as_u64()).unwrap_or(0) as u32;
    let exit_code = state_val
        .get("ExitCode")
        .and_then(|e| e.as_i64())
        .unwrap_or(0) as i32;

    let state_str = if running {
        "running".to_string()
    } else if state_val
        .get("Paused")
        .and_then(|p| p.as_bool())
        .unwrap_or(false)
    {
        "paused".to_string()
    } else if state_val
        .get("Restarting")
        .and_then(|r| r.as_bool())
        .unwrap_or(false)
    {
        "restarting".to_string()
    } else {
        "exited".to_string()
    };

    let driver = config_v2
        .get("Driver")
        .and_then(|d| d.as_str())
        .unwrap_or("overlay2")
        .to_string();

    let graph_driver = config_v2
        .get("GraphDriver")
        .and_then(|g| g.get("Data"))
        .cloned()
        .unwrap_or(Value::Null);

    let upper_dir = graph_driver
        .get("UpperDir")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());
    let merged_dir = graph_driver
        .get("MergedDir")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());
    let work_dir = graph_driver
        .get("WorkDir")
        .and_then(|w| w.as_str())
        .map(|s| s.to_string());

    let log_path = config_v2
        .get("LogPath")
        .and_then(|l| l.as_str())
        .map(|s| s.to_string())
        .or_else(|| container_path.map(|p| format!("{}/{}-json.log", p, container_id)));

    let ip_address = config_v2
        .get("NetworkSettings")
        .and_then(|n| n.get("IPAddress"))
        .and_then(|ip| ip.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut ports = Vec::new();
    if let Some(port_map) = effective_host_config
        .get("PortBindings")
        .and_then(|pb| pb.as_object())
    {
        for (container_port, bindings) in port_map {
            if let Some(bindings_arr) = bindings.as_array() {
                for b in bindings_arr {
                    let host_port = b.get("HostPort").and_then(|p| p.as_str()).unwrap_or("");
                    let host_ip = b.get("HostIp").and_then(|i| i.as_str()).unwrap_or("");
                    ports.push(format!(
                        "{}:{}/{} -> {}",
                        if host_ip.is_empty() {
                            "0.0.0.0"
                        } else {
                            host_ip
                        },
                        host_port,
                        container_port,
                        container_port
                    ));
                }
            }
        }
    }

    let mut mounts = Vec::new();
    if let Some(mount_arr) = config_v2.get("MountPoints").and_then(|m| m.as_object()) {
        for (_, mval) in mount_arr {
            let source = mval
                .get("Source")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let destination = mval
                .get("Destination")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let mode = mval
                .get("Mode")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let rw = mval.get("RW").and_then(|r| r.as_bool()).unwrap_or(true);
            let propagation = mval
                .get("Propagation")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            mounts.push(ContainerMount {
                source,
                destination,
                mode,
                rw,
                propagation,
            });
        }
    } else if let Some(mount_arr) = config_v2.get("Mounts").and_then(|m| m.as_array()) {
        for mval in mount_arr {
            let source = mval
                .get("Source")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let destination = mval
                .get("Destination")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let mode = mval
                .get("Mode")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let rw = mval.get("RW").and_then(|r| r.as_bool()).unwrap_or(true);
            let propagation = mval
                .get("Propagation")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            mounts.push(ContainerMount {
                source,
                destination,
                mode,
                rw,
                propagation,
            });
        }
    }

    let mut env_list = Vec::new();
    if let Some(env_arr) = config_v2
        .get("Config")
        .and_then(|c| c.get("Env"))
        .and_then(|e| e.as_array())
    {
        for item in env_arr {
            if let Some(s) = item.as_str() {
                env_list.push(s.to_string());
            }
        }
    }

    let secrets_found = scan_env_for_secrets(&env_list);
    let (risk_level, risk_reasons) =
        evaluate_container_risk(config_v2, effective_host_config, &mounts);

    let privileged = effective_host_config
        .get("Privileged")
        .and_then(|p| p.as_bool())
        .unwrap_or(false);

    DockerContainerSummary {
        id: container_id.to_string(),
        short_id,
        name: if name.is_empty() {
            "unnamed".to_string()
        } else {
            name
        },
        image,
        created,
        state: state_str,
        running,
        pid,
        exit_code,
        upper_dir,
        merged_dir,
        work_dir,
        log_path,
        ip_address,
        ports,
        privileged,
        risk_level,
        risk_reasons,
        mounts,
        secrets_found,
        driver,
    }
}

/// Docker komut satırı arayüzünden (CLI) canlı konteyner listesini çeker.
pub fn list_containers_from_cli() -> AmeleResult<Vec<DockerContainerSummary>> {
    let output = std::process::Command::new("docker")
        .args(["ps", "-aq"])
        .output()
        .map_err(|e| AmeleError::io(HataKodu::DosyaOkuma, "docker ps komutu çalıştırılamadı", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let ids_str = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<&str> = ids_str.split_whitespace().collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let inspect_output = std::process::Command::new("docker")
        .arg("inspect")
        .args(&ids)
        .output()
        .map_err(|e| {
            AmeleError::io(
                HataKodu::DosyaOkuma,
                "docker inspect komutu çalıştırılamadı",
                e,
            )
        })?;

    if !inspect_output.status.success() {
        return Ok(Vec::new());
    }

    let inspect_json: Value = serde_json::from_slice(&inspect_output.stdout).map_err(|e| {
        AmeleError::new(
            HataKodu::IcerikGecersiz,
            format!("docker inspect çıktısı JSON olarak ayrıştırılamadı: {e}"),
        )
    })?;

    let mut list = Vec::new();
    if let Some(arr) = inspect_json.as_array() {
        for val in arr {
            let cid = val.get("Id").and_then(|i| i.as_str()).unwrap_or("");
            if !cid.is_empty() {
                let summary = parse_single_container(val, &Value::Null, cid, None);
                list.push(summary);
            }
        }
    }

    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(list)
}

/// Belirli bir konteynerin log dosyasını okur ve JSON kayıtları olarak döndürür.
pub fn get_container_logs(
    container_id: &str,
    tail: usize,
    custom_root: Option<&Path>,
) -> AmeleResult<Vec<Value>> {
    let root_path = custom_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DOCKER_ROOT));

    let log_file = root_path
        .join("containers")
        .join(container_id)
        .join(format!("{}-json.log", container_id));

    let mut logs = Vec::new();

    if log_file.exists() {
        if let Ok(file) = File::open(&log_file) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    logs.push(val);
                }
            }
        }
    }

    if logs.is_empty() && custom_root.is_none() {
        let tail_arg = if tail > 0 {
            tail.to_string()
        } else {
            "200".to_string()
        };
        if let Ok(output) = std::process::Command::new("docker")
            .args(["logs", "--tail", &tail_arg, container_id])
            .output()
        {
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                let stderr_str = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}{}", stdout_str, stderr_str);
                for line in combined.lines() {
                    if !line.is_empty() {
                        logs.push(json!({
                            "log": format!("{}\n", line),
                            "stream": "cli",
                            "time": Local::now().to_rfc3339()
                        }));
                    }
                }
            }
        }
    }

    if tail > 0 && logs.len() > tail {
        Ok(logs.split_off(logs.len() - tail))
    } else {
        Ok(logs)
    }
}

/// Konteyner delillerini (Overlay2 diff, konfigürasyon, loglar ve metadata) vaka klasörüne toplar.
pub fn acquire_container_evidence<F>(
    req: &DockerAcquisitionRequest,
    base_case_dir: impl AsRef<Path>,
    mut progress_callback: F,
) -> AmeleResult<DockerAcquisitionResult>
where
    F: FnMut(&str, u64, u64),
{
    let root_path = req
        .custom_docker_root
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DOCKER_ROOT));

    let container_dir = root_path.join("containers").join(&req.container_id);
    let config_path = container_dir.join("config.v2.json");
    let hostconfig_path = container_dir.join("hostconfig.json");

    let (config_v2, host_config) = if config_path.exists() {
        let cv2 = fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or(Value::Null);
        let hc = if hostconfig_path.exists() {
            fs::read_to_string(&hostconfig_path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        (cv2, hc)
    } else if req.custom_docker_root.is_none() {
        if let Ok(output) = std::process::Command::new("docker")
            .args(["inspect", &req.container_id])
            .output()
        {
            if output.status.success() {
                if let Ok(Value::Array(arr)) = serde_json::from_slice::<Value>(&output.stdout) {
                    if let Some(first) = arr.into_iter().next() {
                        let hc = first.get("HostConfig").cloned().unwrap_or(Value::Null);
                        (first, hc)
                    } else {
                        return Err(AmeleError::new(
                            HataKodu::DosyaAcilamadi,
                            format!("Konteyner bulunamadı: {}", req.container_id),
                        ));
                    }
                } else {
                    return Err(AmeleError::new(
                        HataKodu::DosyaAcilamadi,
                        format!("Konteyner bulunamadı: {}", req.container_id),
                    ));
                }
            } else {
                return Err(AmeleError::new(
                    HataKodu::DosyaAcilamadi,
                    format!("Konteyner bulunamadı: {}", req.container_id),
                ));
            }
        } else {
            return Err(AmeleError::new(
                HataKodu::DosyaAcilamadi,
                format!("Konteyner dizini bulunamadı: {}", container_dir.display()),
            ));
        }
    } else {
        return Err(AmeleError::new(
            HataKodu::DosyaAcilamadi,
            format!("Konteyner dizini bulunamadı: {}", container_dir.display()),
        ));
    };

    let container_name = config_v2
        .get("Name")
        .and_then(|n| n.as_str())
        .unwrap_or("unnamed")
        .trim_start_matches('/')
        .to_string();

    let short_id = if req.container_id.len() >= 12 {
        &req.container_id[..12]
    } else {
        &req.container_id
    };

    let case_name = req
        .case_name
        .clone()
        .unwrap_or_else(|| format!("VAKA_DOCKER_{}", Local::now().format("%Y%m%d_%H%M%S")));

    let vault = EvidenceVault::create(base_case_dir, &case_name)?;
    let target_case_dir = vault
        .case_dir
        .join("docker")
        .join(format!("{}_{}", container_name, short_id));

    fs::create_dir_all(&target_case_dir).map_err(|e| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            format!(
                "Hedef Docker delil klasörü oluşturulamadı: {}",
                target_case_dir.display()
            ),
            e,
        )
    })?;

    let mut files_acquired = Vec::new();
    let job_id = format!("DOCKER_{}", Local::now().format("%Y%m%d_%H%M%S"));

    progress_callback("Konteyner meta verileri kaydediliyor...", 10, 100);

    let meta_file = target_case_dir.join("docker_metadata.json");
    let full_metadata = json!({
        "edinim_zamani": Local::now().to_rfc3339(),
        "konteyner_id": req.container_id,
        "kisa_id": short_id,
        "isim": container_name,
        "config_v2": config_v2,
        "host_config": host_config,
    });

    fs::write(
        &meta_file,
        serde_json::to_string_pretty(&full_metadata).unwrap_or_default(),
    )
    .map_err(|e| {
        AmeleError::io(
            HataKodu::DosyaYazma,
            "docker_metadata.json kaydedilemedi",
            e,
        )
    })?;
    files_acquired.push("docker_metadata.json".to_string());

    if req.acquire_config {
        progress_callback("Konfigürasyon dosyaları kopyalanıyor...", 25, 100);
        if config_path.exists() {
            let dest = target_case_dir.join("config.v2.json");
            let _ = fs::copy(&config_path, &dest);
            files_acquired.push("config.v2.json".to_string());
        }
        if hostconfig_path.exists() {
            let dest = target_case_dir.join("hostconfig.json");
            let _ = fs::copy(&hostconfig_path, &dest);
            files_acquired.push("hostconfig.json".to_string());
        }
    }

    let mut logs_saved = false;
    if req.acquire_logs {
        progress_callback("Konteyner günlükleri kopyalanıyor...", 40, 100);
        let log_file = container_dir.join(format!("{}-json.log", req.container_id));
        if log_file.exists() {
            let dest = target_case_dir.join("container.log");
            if fs::copy(&log_file, &dest).is_ok() {
                files_acquired.push("container.log".to_string());
                logs_saved = true;
            }
        }
        if !logs_saved && req.custom_docker_root.is_none() {
            if let Ok(output) = std::process::Command::new("docker")
                .args(["logs", &req.container_id])
                .output()
            {
                if output.status.success() {
                    let dest = target_case_dir.join("container.log");
                    let mut log_bytes = output.stdout;
                    log_bytes.extend_from_slice(&output.stderr);
                    if fs::write(&dest, log_bytes).is_ok() {
                        files_acquired.push("container.log".to_string());
                        logs_saved = true;
                    }
                }
            }
        }
    }

    let mut diff_tar_path = None;
    let mut diff_sha256 = None;
    let mut diff_size_bytes = 0;

    if req.acquire_diff {
        progress_callback(
            "Overlay2 UpperDir (Drift) katmanı tespit ediliyor...",
            55,
            100,
        );

        let upper_dir_path = config_v2
            .get("GraphDriver")
            .and_then(|g| g.get("Data"))
            .and_then(|d| d.get("UpperDir"))
            .and_then(|u| u.as_str())
            .map(PathBuf::from);

        if let Some(upper_dir) = upper_dir_path {
            if upper_dir.exists() && upper_dir.is_dir() {
                progress_callback(
                    "Overlay2 UpperDir dosyaları .tar.gz olarak arşivleniyor...",
                    70,
                    100,
                );

                let tar_gz_file = target_case_dir.join("upper_drift_files.tar.gz");
                let out_file = File::create(&tar_gz_file).map_err(|e| {
                    AmeleError::io(
                        HataKodu::DosyaYazma,
                        format!("Arşiv dosyası oluşturulamadı: {}", tar_gz_file.display()),
                        e,
                    )
                })?;

                let enc = GzEncoder::new(out_file, Compression::default());
                let mut tar_builder = Builder::new(enc);

                if tar_builder.append_dir_all(".", &upper_dir).is_ok() {
                    if let Ok(mut enc_inner) = tar_builder.into_inner() {
                        let _ = enc_inner.try_finish();
                    }

                    if tar_gz_file.exists() {
                        diff_size_bytes = fs::metadata(&tar_gz_file).map(|m| m.len()).unwrap_or(0);
                        if let Ok(hash) = calculate_file_hash(&tar_gz_file, HashAlgorithm::Sha256) {
                            diff_sha256 = Some(hash);
                        }
                        diff_tar_path = Some(tar_gz_file.to_string_lossy().to_string());
                        files_acquired.push("upper_drift_files.tar.gz".to_string());
                    }
                }
            }
        }
    }

    progress_callback(
        "Bütünlük manifestosu (manifest.csv) oluşturuluyor...",
        90,
        100,
    );

    let manifest_path = target_case_dir.join("manifest.csv");
    let mut manifest_content = String::from("Dosya_Adi,Boyut_Byte,SHA256\n");
    for file_name in &files_acquired {
        let fpath = target_case_dir.join(file_name);
        if fpath.exists() {
            let size = fs::metadata(&fpath).map(|m| m.len()).unwrap_or(0);
            let sha = calculate_file_hash(&fpath, HashAlgorithm::Sha256)
                .unwrap_or_else(|_| "HATA".to_string());
            manifest_content.push_str(&format!("{},{},{}\n", file_name, size, sha));
        }
    }
    let _ = fs::write(&manifest_path, manifest_content);

    progress_callback("Docker adli edinimi tamamlandı!", 100, 100);

    Ok(DockerAcquisitionResult {
        job_id,
        container_id: req.container_id.clone(),
        container_name,
        case_path: target_case_dir.to_string_lossy().to_string(),
        diff_tar_path,
        diff_sha256,
        diff_size_bytes,
        config_saved: req.acquire_config,
        logs_saved,
        metadata_saved: true,
        files_acquired,
        message: "Docker konteyner delilleri başarıyla toplandı ve doğrulandı.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_detection_in_env() {
        let envs = vec![
            "PATH=/usr/local/sbin:/usr/local/bin".to_string(),
            "DB_PASSWORD=SuperSecretPass123!".to_string(),
            "API_KEY=ak_live_83921839218".to_string(),
            "NODE_ENV=production".to_string(),
            "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        ];

        let found = scan_env_for_secrets(&envs);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].key, "DB_PASSWORD");
        assert_eq!(found[1].key, "API_KEY");
        assert_eq!(found[2].key, "AWS_SECRET_ACCESS_KEY");
    }

    #[test]
    fn test_risk_evaluation_privileged_and_docker_sock() {
        let config_v2 = json!({
            "Config": { "User": "root" }
        });
        let host_config = json!({
            "Privileged": true,
            "NetworkMode": "host"
        });
        let mounts = vec![ContainerMount {
            source: "/var/run/docker.sock".to_string(),
            destination: "/var/run/docker.sock".to_string(),
            mode: "rw".to_string(),
            rw: true,
            propagation: "rprivate".to_string(),
        }];

        let (level, reasons) = evaluate_container_risk(&config_v2, &host_config, &mounts);
        assert_eq!(level, RiskLevel::Critical);
        assert!(reasons.len() >= 2);
    }
}
