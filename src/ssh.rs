//! SSH / WinRM üzerinden agent'sız Linux ve Windows uzak disk ve RAM edinim motoru.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use chrono::Local;
use digest::Digest;
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use ssh2::Session;

use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::output_format::{self, AcquisitionOutputFormat};
use crate::remote::{RemoteDisk, RemoteToolStatus, RemoteTransferResult};
use crate::settings::DEFAULT_CHUNK_SIZE;

const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConnectionParams {
    pub ip: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
}

pub struct SshConnection {
    pub host: String,
    pub port: u16,
    pub user: String,
    session: Session,
}

impl SshConnection {
    /// SSH sunucusuna (Linux veya Windows OpenSSH) bağlanır ve kimlik doğrulamasını yapar.
    pub fn connect(params: &SshConnectionParams) -> AmeleResult<Self> {
        let port = if params.port == 0 { 22 } else { params.port };
        let addr = (params.ip.as_str(), port)
            .to_socket_addrs()
            .map_err(|err| AmeleError::io(HataKodu::Baglanti, "SSH adres çözümlenemedi", err))?
            .next()
            .ok_or_else(|| AmeleError::new(HataKodu::Baglanti, "SSH adresi bulunamadı"))?;

        let stream = TcpStream::connect_timeout(&addr, SSH_CONNECT_TIMEOUT).map_err(|err| {
            AmeleError::io(HataKodu::Baglanti, "SSH TCP bağlantısı başarısız", err)
        })?;

        let mut session = Session::new().map_err(|err| {
            AmeleError::new(
                HataKodu::Baglanti,
                format!("SSH oturumu oluşturulamadı: {err}"),
            )
        })?;

        session.set_tcp_stream(stream);
        session.handshake().map_err(|err| {
            AmeleError::new(
                HataKodu::Baglanti,
                format!("SSH el sıkışması başarısız: {err}"),
            )
        })?;

        let mut authenticated = false;

        // 1. Anahtar dosyası verilmişse anahtar ile dene
        if let Some(ref key_path_str) = params.key_path {
            let key_path = Path::new(key_path_str);
            if key_path.exists() {
                let passphrase = params.password.as_deref();
                if session
                    .userauth_pubkey_file(&params.user, None, key_path, passphrase)
                    .is_ok()
                {
                    authenticated = true;
                }
            }
        }

        // 2. Parola ile dene
        if !authenticated {
            if let Some(ref pass) = params.password {
                if session.userauth_password(&params.user, pass).is_ok() {
                    authenticated = true;
                }
            }
        }

        // 3. Agent (ssh-agent) ile dene
        if !authenticated && session.userauth_agent(&params.user).is_ok() {
            authenticated = true;
        }

        if !authenticated || !session.authenticated() {
            return Err(AmeleError::new(
                HataKodu::Guvenlik,
                "SSH kimlik doğrulaması başarısız oldu (kullanıcı adı, parola veya anahtarı kontrol edin)",
            ));
        }

        Ok(Self {
            host: params.ip.clone(),
            port,
            user: params.user.clone(),
            session,
        })
    }

    /// SSH oturumu üzerinden tek satırlık bir komut çalıştırıp standart çıktısını döndürür.
    pub fn exec_command(&mut self, cmd: &str) -> AmeleResult<String> {
        let mut channel = self.session.channel_session().map_err(|err| {
            AmeleError::new(HataKodu::AgGonderme, format!("SSH kanalı açılamadı: {err}"))
        })?;

        channel.exec(cmd).map_err(|err| {
            AmeleError::new(
                HataKodu::AgGonderme,
                format!("SSH komut çalıştırma hatası: {err}"),
            )
        })?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|err| AmeleError::io(HataKodu::AgAlma, "SSH çıktısı okunamadı", err))?;

        channel.wait_close().map_err(|err| {
            AmeleError::new(HataKodu::AgAlma, format!("SSH kanal kapatma hatası: {err}"))
        })?;

        Ok(output)
    }

    /// Uzak Linux veya Windows hedefindeki diskleri sorgular.
    pub fn list_disks(&mut self) -> AmeleResult<Vec<RemoteDisk>> {
        let mut disks = Vec::new();

        // 1. Linux lsblk sorgusu
        let cmd_linux = "lsblk -J -b -o NAME,PATH,SIZE,TYPE,MODEL,ROTA 2>/dev/null || lsblk -l -b -o NAME,PATH,SIZE,TYPE 2>/dev/null";
        if let Ok(output) = self.exec_command(cmd_linux) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&output) {
                if let Some(devices) = parsed.get("blockdevices").and_then(Value::as_array) {
                    for dev in devices {
                        let dev_type = dev.get("type").and_then(Value::as_str).unwrap_or("");
                        if dev_type == "disk" || dev_type == "loop" || dev_type.is_empty() {
                            let name = dev
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let path = dev
                                .get("path")
                                .and_then(Value::as_str)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("/dev/{name}"));
                            let size = dev.get("size").and_then(Value::as_u64).unwrap_or(0);
                            let model = dev
                                .get("model")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .trim();
                            let display_name = if !model.is_empty() {
                                format!("{name} ({model})")
                            } else {
                                name.clone()
                            };

                            if !name.is_empty() && size > 0 {
                                disks.push(RemoteDisk {
                                    id: path,
                                    ad: display_name,
                                    boyut: size,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. Linux /proc/partitions yedek
        if disks.is_empty() {
            if let Ok(partitions_output) = self.exec_command("cat /proc/partitions 2>/dev/null") {
                for line in partitions_output.lines().skip(2) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let blocks: u64 = parts[2].parse().unwrap_or(0);
                        let name = parts[3].to_string();
                        if !name.contains("loop") && !name.contains("ram") {
                            disks.push(RemoteDisk {
                                id: format!("/dev/{name}"),
                                ad: name,
                                boyut: blocks * 1024,
                            });
                        }
                    }
                }
            }
        }

        // 3. Windows PowerShell / WMIC sorgusu (Windows SSH bağlantısı için)
        if disks.is_empty() {
            let win_cmd = "powershell -NoProfile -Command \"Get-CimInstance Win32_DiskDrive | Select-Object DeviceID,Model,Size | ConvertTo-Json -Compress\" 2>$null";
            if let Ok(win_out) = self.exec_command(win_cmd) {
                if let Ok(val) = serde_json::from_str::<Value>(&win_out) {
                    let items = if let Some(arr) = val.as_array() {
                        arr.clone()
                    } else if val.is_object() {
                        vec![val]
                    } else {
                        vec![]
                    };
                    for item in items {
                        let dev_id = item
                            .get("DeviceID")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let model = item
                            .get("Model")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim();
                        let size = item.get("Size").and_then(Value::as_u64).unwrap_or(0);
                        let clean_id = dev_id.replace(r"\\.\", "").replace(r"//./", "");
                        let display = if !model.is_empty() {
                            format!("{clean_id} ({model})")
                        } else {
                            clean_id.clone()
                        };
                        if !clean_id.is_empty() {
                            disks.push(RemoteDisk {
                                id: clean_id,
                                ad: display,
                                boyut: size,
                            });
                        }
                    }
                }
            }
        }

        Ok(disks)
    }

    /// Uzak sistemdeki RAM araçlarını (AVML / WinPMEM / kcore) ve yetkileri kontrol eder.
    pub fn check_ram_tools(&mut self) -> AmeleResult<RemoteToolStatus> {
        let check_cmd = r#"
            which avml 2>/dev/null || where winpmem 2>nul || echo "NO_TOOL"
            [ -r /proc/kcore ] && echo "KCORE_OK" || echo "NO_KCORE"
            id -u 2>/dev/null || whoami
            grep MemTotal /proc/meminfo 2>/dev/null | awk '{print $2}' || powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).TotalVisibleMemorySize" 2>$null
        "#;
        let out = self.exec_command(check_cmd)?;
        let lines: Vec<&str> = out
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let tool_path_raw = lines.first().copied().unwrap_or("NO_TOOL");
        let has_tool = tool_path_raw != "NO_TOOL" && !tool_path_raw.is_empty();
        let has_kcore = lines.get(1).copied().unwrap_or("NO_KCORE") == "KCORE_OK";
        let user_id_or_name = lines.get(2).copied().unwrap_or("");
        let is_admin = user_id_or_name == "0"
            || user_id_or_name.to_lowercase().contains("admin")
            || user_id_or_name.to_lowercase().contains("system");
        let ram_kb: u64 = lines.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let ram_bytes = ram_kb * 1024;

        let is_windows =
            tool_path_raw.to_lowercase().contains("winpmem") || user_id_or_name.contains('\\');

        let (tool_present, tool_path, message) = if has_tool && is_windows {
            (
                true,
                tool_path_raw.to_string(),
                "WinPMEM aracı hazır".to_string(),
            )
        } else if has_tool {
            (
                true,
                tool_path_raw.to_string(),
                "AVML aracı hazır".to_string(),
            )
        } else if has_kcore {
            (
                true,
                "/proc/kcore".to_string(),
                "/proc/kcore üzerinden edinim hazır".to_string(),
            )
        } else if is_windows {
            (
                true,
                "winpmem".to_string(),
                "Windows WinPMEM hazır".to_string(),
            )
        } else {
            (
                false,
                "Arac bulunamadi".to_string(),
                "AVML veya WinPMEM bulunamadı".to_string(),
            )
        };

        Ok(RemoteToolStatus {
            tool_present,
            admin_privilege: is_admin,
            ram_size: ram_bytes,
            tool_path,
            message,
        })
    }

    /// SSH üzerinden dd / PhysicalDrive ile uzak disk imajı akışını başlatır.
    pub fn acquire_disk<F>(
        &mut self,
        disk_path: &str,
        target_dir: &Path,
        case_name: Option<&str>,
        output_format: AcquisitionOutputFormat,
        mut on_progress: F,
    ) -> AmeleResult<RemoteTransferResult>
    where
        F: FnMut(u64, u64),
    {
        fs::create_dir_all(target_dir).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Hedef klasör oluşturulamadı", err)
        })?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let disk_clean = disk_path
            .replace(['/', '\\', '.'], "_")
            .trim_matches('_')
            .to_string();
        let raw_filename = format!(
            "ssh_{}_{}_{}.raw",
            self.host.replace('.', "_"),
            disk_clean,
            timestamp
        );
        let target_seed = target_dir.join(&raw_filename);

        let plan = output_format::plan_output(&target_seed, output_format);

        let mut channel = self.session.channel_session().map_err(|err| {
            AmeleError::new(HataKodu::AgGonderme, format!("SSH kanalı açılamadı: {err}"))
        })?;

        // Windows PhysicalDrive kontrolü veya Linux /dev disk
        let dd_cmd = if disk_path.to_lowercase().contains("physicaldrive") {
            let win_target = if disk_path.starts_with(r"\\.\") {
                disk_path.to_string()
            } else {
                format!(r"\\.\{disk_path}")
            };
            format!(
                r"dd.exe if={win_target} bs=4M 2>nul || powershell -NoProfile -Command [IO.File]::OpenRead('{win_target}')"
            )
        } else {
            format!(
                "sudo dd if={disk_path} bs=4M status=none 2>/dev/null || dd if={disk_path} bs=4M status=none 2>/dev/null"
            )
        };

        channel.exec(&dd_cmd).map_err(|err| {
            AmeleError::new(
                HataKodu::AgGonderme,
                format!("SSH dd komutu çalıştırılamadı: {err}"),
            )
        })?;

        let mut file = File::create(&plan.working_path).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                "Hedef imaj dosyası oluşturulamadı",
                err,
            )
        })?;

        let mut buffer = vec![0u8; DEFAULT_CHUNK_SIZE];
        let mut total_bytes = 0u64;
        let mut sha256_hasher = Sha256::new();
        let mut md5_hasher = Md5::new();

        loop {
            let read = channel
                .read(&mut buffer)
                .map_err(|err| AmeleError::io(HataKodu::AgAlma, "SSH veri akışı okunamadı", err))?;
            if read == 0 {
                break;
            }
            let chunk = &buffer[..read];
            file.write_all(chunk).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "İmaj dosyasına yazılamadı", err)
            })?;
            sha256_hasher.update(chunk);
            md5_hasher.update(chunk);
            total_bytes += read as u64;
            on_progress(total_bytes, 0);
        }

        file.flush()
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Dosya flush edilemedi", err))?;

        let sha256_str = to_hex(&sha256_hasher.finalize());
        let md5_str = to_hex(&md5_hasher.finalize());

        let finalized = output_format::finalize_output(
            &plan,
            "disk",
            disk_path,
            case_name.unwrap_or_default(),
            Some(sha256_str.clone()),
        )
        .map_err(|err| AmeleError::new(HataKodu::Dosya, err))?;

        Ok(RemoteTransferResult {
            job_id: format!("SSH_DISK_{}", Local::now().format("%Y%m%d%H%M%S")),
            target_path: finalized.target_path,
            bytes_transferred: total_bytes,
            sha256: Some(sha256_str),
            md5: Some(md5_str),
            message: format!("SSH ile {total_bytes} bayt disk imajı başarıyla alındı"),
        })
    }

    /// SSH üzerinden AVML / WinPMEM / /proc/kcore ile RAM dökümü akışını başlatır.
    pub fn acquire_ram<F>(
        &mut self,
        target_dir: &Path,
        case_name: Option<&str>,
        output_format: AcquisitionOutputFormat,
        mut on_progress: F,
    ) -> AmeleResult<RemoteTransferResult>
    where
        F: FnMut(u64, u64),
    {
        fs::create_dir_all(target_dir).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                "Hedef RAM klasörü oluşturulamadı",
                err,
            )
        })?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let raw_filename = format!("ssh_ram_{}_{}.lime", self.host.replace('.', "_"), timestamp);
        let target_seed = target_dir.join(&raw_filename);

        let plan = output_format::plan_output(&target_seed, output_format);

        let mut channel = self.session.channel_session().map_err(|err| {
            AmeleError::new(HataKodu::AgGonderme, format!("SSH kanalı açılamadı: {err}"))
        })?;

        let ram_cmd = "sudo avml /dev/stdout 2>/dev/null || winpmem.exe - 2>nul || winpmem_mini.exe - 2>nul || sudo dd if=/proc/kcore bs=4M status=none 2>/dev/null";
        channel.exec(ram_cmd).map_err(|err| {
            AmeleError::new(
                HataKodu::AgGonderme,
                format!("SSH RAM komutu çalıştırılamadı: {err}"),
            )
        })?;

        let mut file = File::create(&plan.working_path).map_err(|err| {
            AmeleError::io(
                HataKodu::DosyaYazma,
                "Hedef RAM dosyası oluşturulamadı",
                err,
            )
        })?;

        let mut buffer = vec![0u8; DEFAULT_CHUNK_SIZE];
        let mut total_bytes = 0u64;
        let mut sha256_hasher = Sha256::new();
        let mut md5_hasher = Md5::new();

        loop {
            let read = channel
                .read(&mut buffer)
                .map_err(|err| AmeleError::io(HataKodu::AgAlma, "SSH RAM akışı okunamadı", err))?;
            if read == 0 {
                break;
            }
            let chunk = &buffer[..read];
            file.write_all(chunk).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "RAM dosyasına yazılamadı", err)
            })?;
            sha256_hasher.update(chunk);
            md5_hasher.update(chunk);
            total_bytes += read as u64;
            on_progress(total_bytes, 0);
        }

        file.flush().map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "RAM dosyası flush edilemedi", err)
        })?;

        let sha256_str = to_hex(&sha256_hasher.finalize());
        let md5_str = to_hex(&md5_hasher.finalize());

        let finalized = output_format::finalize_output(
            &plan,
            "ram",
            "ssh_ram",
            case_name.unwrap_or_default(),
            Some(sha256_str.clone()),
        )
        .map_err(|err| AmeleError::new(HataKodu::Dosya, err))?;

        Ok(RemoteTransferResult {
            job_id: format!("SSH_RAM_{}", Local::now().format("%Y%m%d%H%M%S")),
            target_path: finalized.target_path,
            bytes_transferred: total_bytes,
            sha256: Some(sha256_str),
            md5: Some(md5_str),
            message: format!("SSH ile {total_bytes} bayt RAM dökümü başarıyla alındı"),
        })
    }
}
