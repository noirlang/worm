//! Linux/Windows agent protokolüyle uzak disk ve RAM edinim bağlantılarını yönetir.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::output_format::AcquisitionOutputFormat;
use crate::settings::DEFAULT_CHUNK_SIZE;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Uzak agent tarafından raporlanan disk kimliği, adı ve boyutunu taşır.
pub struct RemoteDisk {
    pub id: String,
    pub ad: String,
    pub boyut: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Uzak agent tarafındaki AVML/WinPMEM durumunu temsil eder.
pub struct RemoteToolStatus {
    pub tool_present: bool,
    pub admin_privilege: bool,
    pub ram_size: u64,
    pub tool_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Uzak disk/RAM dosyası aktarımının yerel çıktı ve hash sonucunu taşır.
pub struct RemoteTransferResult {
    pub job_id: String,
    pub target_path: PathBuf,
    pub bytes_transferred: u64,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Uzak RAM edinim işi tamamlandığında agent tarafı sonucu taşır.
pub struct RemoteRamResult {
    pub job_id: String,
    pub total_size: u64,
    pub sha256: Option<String>,
    pub message: String,
}

/// Uzak agent ile JSON satır protokolü üzerinden konuşan bağlantı nesnesidir.
pub struct RemoteConnection {
    host: String,
    port: u16,
    token: Option<String>,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    pub server_name: String,
    pub server_version: String,
    pub features: Vec<String>,
    pub last_error: String,
}

impl RemoteConnection {
    /// Agent'a TCP ile bağlanır, hello/token el sıkışmasını yapar.
    pub fn connect(host: impl Into<String>, port: u16, token: Option<String>) -> AmeleResult<Self> {
        let host = host.into();
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|err| AmeleError::io(HataKodu::Baglanti, "Adres cozumlenemedi", err))?
            .next()
            .ok_or_else(|| AmeleError::new(HataKodu::Baglanti, "Adres bulunamadi"))?;
        let stream = TcpStream::connect_timeout(&addr, HELLO_TIMEOUT)
            .map_err(|err| AmeleError::io(HataKodu::Baglanti, "Baglanti basarisiz", err))?;
        stream
            .set_read_timeout(Some(HELLO_TIMEOUT))
            .map_err(|err| {
                AmeleError::io(HataKodu::Baglanti, "Socket timeout ayarlanamadi", err)
            })?;
        stream
            .set_write_timeout(Some(HELLO_TIMEOUT))
            .map_err(|err| {
                AmeleError::io(HataKodu::Baglanti, "Socket timeout ayarlanamadi", err)
            })?;
        let writer = stream
            .try_clone()
            .map_err(|err| AmeleError::io(HataKodu::Baglanti, "Socket clone basarisiz", err))?;
        let mut connection = Self {
            host,
            port,
            token,
            reader: BufReader::new(stream),
            writer,
            server_name: String::new(),
            server_version: String::new(),
            features: Vec::new(),
            last_error: String::new(),
        };
        connection.hello()?;
        Ok(connection)
    }

    /// Bağlı agent IP/host değerini döndürür.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Bağlı agent port değerini döndürür.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Uzak agent üzerindeki diskleri listeler.
    pub fn list_disks(&mut self) -> AmeleResult<Vec<RemoteDisk>> {
        self.last_error.clear();
        self.send_json(&json!({"komut": "disk_listele"}))?;
        let response = self.read_json_line()?;
        if !is_ok(&response) {
            self.last_error = message_from(&response, "Beklenmeyen yanit");
            return Err(AmeleError::new(HataKodu::Protokol, self.last_error.clone()));
        }

        Ok(response
            .get("diskler")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| RemoteDisk {
                        id: item
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        ad: item
                            .get("ad")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        boyut: item
                            .get("boyut")
                            .and_then(Value::as_u64)
                            .unwrap_or_default(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default())
    }

    /// Uzak agent üzerinden disk imajı başlatır ve binary akışı yerel dosyaya yazar.
    pub fn acquire_image<F>(
        &mut self,
        disk_id: &str,
        disk_name: Option<&str>,
        target_dir: impl AsRef<Path>,
        job_id: Option<&str>,
        format: AcquisitionOutputFormat,
        mut progress: F,
    ) -> AmeleResult<RemoteTransferResult>
    where
        F: FnMut(u64, u64),
    {
        fs::create_dir_all(target_dir.as_ref()).map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Hedef klasor olusturulamadi", err)
        })?;
        let file_name = canonical_image_file_name(Some(&self.host), disk_id, disk_name);
        let target_path = target_dir.as_ref().join(file_name);
        let mut target = File::create(&target_path)
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Hedef dosya acilamadi", err))?;

        let mut request = json!({
            "komut": "imaj_baslat",
            "disk_id": disk_id,
            "format": format.as_str(),
            "parca_boyutu": DEFAULT_CHUNK_SIZE,
        });
        if let Some(job_id) = job_id {
            request["is_id"] = Value::String(job_id.to_string());
        }
        self.send_json(&request)?;

        let start_response = self.read_json_line()?;
        if !is_ok(&start_response) {
            let message = message_from(&start_response, "Imaj baslatilamadi");
            drop(target);
            mark_partial(&target_path)?;
            return Err(AmeleError::new(HataKodu::Protokol, message));
        }

        let mut transfer_job_id = start_response
            .get("is_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut total = start_response
            .get("tahmini_boyut")
            .and_then(Value::as_u64)
            .unwrap_or_default();

        loop {
            let event = self.read_json_line()?;
            if event.get("tur").and_then(Value::as_str) == Some("veri_basliyor") {
                if transfer_job_id.is_empty() {
                    transfer_job_id = event
                        .get("is_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
                total = event
                    .get("toplam")
                    .and_then(Value::as_u64)
                    .filter(|value| *value > 0)
                    .unwrap_or(total);
                break;
            }
            if event.get("tur").and_then(Value::as_str) == Some("hata") || is_error(&event) {
                let message = message_from(&event, "Uzak ajan imaj hatasi");
                drop(target);
                mark_partial(&target_path)?;
                return Err(AmeleError::new(HataKodu::Protokol, message));
            }
            if event.get("tur").and_then(Value::as_str) == Some("ilerleme") {
                let done = event
                    .get("okunan")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                progress(done, total);
            }
        }

        if total == 0 {
            drop(target);
            mark_partial(&target_path)?;
            return Err(AmeleError::new(
                HataKodu::DiskBoyut,
                "Uzak disk boyutu alinamadi",
            ));
        }

        self.reader
            .get_ref()
            .set_read_timeout(None)
            .map_err(|err| {
                AmeleError::io(HataKodu::Baglanti, "Socket timeout kapatilamadi", err)
            })?;

        let mut transferred = 0_u64;
        let mut buffer = vec![0_u8; DEFAULT_CHUNK_SIZE];
        while transferred < total {
            let to_read = (total - transferred).min(buffer.len() as u64) as usize;
            let read = self
                .reader
                .read(&mut buffer[..to_read])
                .map_err(|err| AmeleError::io(HataKodu::AgAlma, "Uzak veri okunamadi", err))?;
            if read == 0 {
                drop(target);
                mark_partial(&target_path)?;
                return Err(AmeleError::new(
                    HataKodu::BaglantiKesildi,
                    "Ajan baglantisi kesildi",
                ));
            }
            target
                .write_all(&buffer[..read])
                .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Uzak veri yazilamadi", err))?;
            transferred += read as u64;
            progress(transferred, total);
        }
        target.flush().map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Hedef dosya flush edilemedi", err)
        })?;
        drop(target);

        let end = self.read_json_line()?;
        if end.get("tur").and_then(Value::as_str) != Some("bitti") {
            mark_partial(&target_path)?;
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&end, "Bitis mesaji alinamadi"),
            ));
        }

        Ok(RemoteTransferResult {
            job_id: transfer_job_id,
            target_path,
            bytes_transferred: transferred,
            sha256: end
                .get("sha256")
                .and_then(Value::as_str)
                .map(str::to_string),
            md5: end.get("md5").and_then(Value::as_str).map(str::to_string),
            message: message_from(&end, "Imaj alma tamamlandi"),
        })
    }

    /// Uzak Windows agent üstünde WinPMEM durumunu kontrol eder.
    pub fn check_winpmem(&mut self) -> AmeleResult<RemoteToolStatus> {
        self.send_json(&json!({"komut": "winpmem_kontrol"}))?;
        let response = self.read_json_line()?;
        parse_tool_status(&response, "winpmem_mevcut", "winpmem_yol")
    }

    /// Uzak Linux agent üstünde AVML durumunu kontrol eder.
    pub fn check_avml(&mut self) -> AmeleResult<RemoteToolStatus> {
        self.send_json(&json!({"komut": "avml_kontrol"}))?;
        let response = self.read_json_line()?;
        parse_tool_status(&response, "avml_mevcut", "avml_yol")
    }

    /// Uzak agent üzerinde RAM edinim işini başlatır ve ilerlemeyi izler.
    pub fn start_remote_ram<F>(
        &mut self,
        output_file: &str,
        job_id: Option<&str>,
        format: AcquisitionOutputFormat,
        mut progress: F,
    ) -> AmeleResult<RemoteRamResult>
    where
        F: FnMut(u64, u64),
    {
        let mut request = json!({
            "komut": "ram_edinim_baslat",
            "cikti_dosya": output_file,
            "format": format.as_str(),
        });
        if let Some(job_id) = job_id {
            request["is_id"] = Value::String(job_id.to_string());
        }
        self.reader
            .get_ref()
            .set_read_timeout(None)
            .map_err(|err| {
                AmeleError::io(HataKodu::Baglanti, "Socket timeout kapatilamadi", err)
            })?;
        self.send_json(&request)?;

        let mut actual_job_id = job_id.unwrap_or_default().to_string();
        let mut total = 0_u64;
        let mut data_started = false;

        loop {
            let event = self.read_json_line()?;
            if is_error(&event) || event.get("tur").and_then(Value::as_str) == Some("hata") {
                return Err(AmeleError::new(
                    HataKodu::Protokol,
                    message_from(&event, "Uzak RAM edinim hatasi"),
                ));
            }

            if is_ok(&event) {
                total = event
                    .get("toplam_boyut")
                    .and_then(Value::as_u64)
                    .unwrap_or(total);
                if actual_job_id.is_empty() {
                    actual_job_id = event
                        .get("is_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
                continue;
            }

            match event.get("tur").and_then(Value::as_str).unwrap_or_default() {
                "veri_basliyor" => {
                    data_started = true;
                    total = event.get("toplam").and_then(Value::as_u64).unwrap_or(total);
                    if actual_job_id.is_empty() {
                        actual_job_id = event
                            .get("is_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                    }
                }
                "ilerleme" if data_started => {
                    let done = event
                        .get("okunan")
                        .and_then(Value::as_u64)
                        .unwrap_or_default();
                    let event_total = event.get("toplam").and_then(Value::as_u64).unwrap_or(total);
                    progress(done, event_total);
                }
                "bitti" => {
                    let final_size = event.get("boyut").and_then(Value::as_u64).unwrap_or(total);
                    if final_size > 0 {
                        progress(final_size, final_size);
                    }
                    return Ok(RemoteRamResult {
                        job_id: actual_job_id,
                        total_size: final_size,
                        sha256: event
                            .get("sha256")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        message: message_from(&event, "RAM edinimi tamamlandi"),
                    });
                }
                _ => {}
            }
        }
    }

    /// Agent tarafında üretilmiş RAM dosyasını yerel hedefe indirir.
    pub fn download_ram_file<F>(
        &mut self,
        remote_file: &str,
        local_path: impl AsRef<Path>,
        job_id: Option<&str>,
        mut progress: F,
    ) -> AmeleResult<RemoteTransferResult>
    where
        F: FnMut(u64, u64),
    {
        let local_path = local_path.as_ref();
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "Hedef klasor olusturulamadi", err)
            })?;
        }

        let mut request = json!({
            "komut": "ram_dosya_indir",
            "dosya": remote_file,
        });
        if let Some(job_id) = job_id {
            request["is_id"] = Value::String(job_id.to_string());
        }
        self.send_json(&request)?;

        let start = self.read_json_line()?;
        if !is_ok(&start) {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&start, "RAM dosyasi indirilemedi"),
            ));
        }

        let mut job_id = start
            .get("is_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut total = start
            .get("tahmini_boyut")
            .and_then(Value::as_u64)
            .unwrap_or_default();

        let data_start = self.read_json_line()?;
        if data_start.get("tur").and_then(Value::as_str) != Some("veri_basliyor") {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&data_start, "Veri baslangici alinamadi"),
            ));
        }
        if job_id.is_empty() {
            job_id = data_start
                .get("is_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
        total = data_start
            .get("toplam")
            .and_then(Value::as_u64)
            .unwrap_or(total);

        let mut output = File::create(local_path)
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Yerel dosya acilamadi", err))?;
        let mut transferred = 0_u64;
        let mut buffer = vec![0_u8; DEFAULT_CHUNK_SIZE];
        while transferred < total {
            let to_read = (total - transferred).min(buffer.len() as u64) as usize;
            let read = self
                .reader
                .read(&mut buffer[..to_read])
                .map_err(|err| AmeleError::io(HataKodu::AgAlma, "RAM dosyasi okunamadi", err))?;
            if read == 0 {
                drop(output);
                mark_partial(local_path)?;
                return Err(AmeleError::new(
                    HataKodu::BaglantiKesildi,
                    "Ajan baglantisi kesildi",
                ));
            }
            output
                .write_all(&buffer[..read])
                .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Yerel yazma hatasi", err))?;
            transferred += read as u64;
            progress(transferred, total);
        }
        output
            .flush()
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Yerel flush hatasi", err))?;
        drop(output);

        let end = self.read_json_line()?;
        if end.get("tur").and_then(Value::as_str) != Some("bitti") {
            mark_partial(local_path)?;
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&end, "RAM indirme bitis mesaji alinamadi"),
            ));
        }

        Ok(RemoteTransferResult {
            job_id,
            target_path: local_path.to_path_buf(),
            bytes_transferred: transferred,
            sha256: end
                .get("sha256")
                .and_then(Value::as_str)
                .map(str::to_string),
            md5: None,
            message: message_from(&end, "RAM dosyasi indirildi"),
        })
    }

    /// Uzak agent üzerindeki iş için pause/resume/stop kontrol komutu gönderir.
    pub fn control_job(&self, job_id: &str, action: &str) -> AmeleResult<String> {
        let mut control = Self::connect(self.host.clone(), self.port, self.token.clone())?;
        control.send_json(&json!({
            "komut": "edinim_kontrol",
            "is_id": job_id,
            "eylem": action,
        }))?;
        let response = control.read_json_line()?;
        let message = message_from(&response, "Kontrol komutu uygulandi");
        if is_ok(&response) {
            Ok(message)
        } else {
            Err(AmeleError::new(HataKodu::Protokol, message))
        }
    }

    /// Uzak agent tarafındaki Docker sistem durumunu sorgular.
    pub fn docker_status(&mut self) -> AmeleResult<crate::docker::DockerSystemStatus> {
        self.send_json(&json!({ "komut": "docker_durum" }))?;
        let response = self.read_json_line()?;
        if !is_ok(&response) {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&response, "Docker durumu sorgulanamadi"),
            ));
        }

        let is_avail = response
            .get("docker_mevcut")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_running = response
            .get("docker_calisiyor")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let storage_driver = response
            .get("depolama_surucusu")
            .and_then(Value::as_str)
            .unwrap_or("overlay2")
            .to_string();
        let root_dir = response
            .get("kok_dizin")
            .and_then(Value::as_str)
            .unwrap_or("/var/lib/docker")
            .to_string();
        let count = response
            .get("konteyner_sayisi")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let running = response
            .get("calisan_sayisi")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let paused = response
            .get("duraklatilan_sayisi")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let stopped = response
            .get("durdurulan_sayisi")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let message = message_from(&response, "Docker durumu alindi");

        Ok(crate::docker::DockerSystemStatus {
            docker_available: is_avail,
            docker_running: is_running,
            storage_driver,
            root_dir,
            containers_count: count,
            running_count: running,
            paused_count: paused,
            stopped_count: stopped,
            is_offline: false,
            message,
        })
    }

    /// Uzak agent üzerindeki Docker konteynerlerini listeler ve güvenlik analizini çeker.
    pub fn list_docker_containers(
        &mut self,
    ) -> AmeleResult<Vec<crate::docker::DockerContainerSummary>> {
        self.send_json(&json!({ "komut": "docker_listele" }))?;
        let response = self.read_json_line()?;
        if !is_ok(&response) {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&response, "Docker konteynerleri listelenemedi"),
            ));
        }

        let containers = response
            .get("konteynerler")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut list = Vec::new();
        for item in containers {
            if let Ok(summary) =
                serde_json::from_value::<crate::docker::DockerContainerSummary>(item)
            {
                list.push(summary);
            }
        }
        Ok(list)
    }

    /// Uzak agent üzerindeki belirli bir konteynerin loglarını çeker.
    pub fn get_docker_container_logs(
        &mut self,
        container_id: &str,
        tail: usize,
    ) -> AmeleResult<Vec<Value>> {
        self.send_json(&json!({
            "komut": "docker_loglar",
            "konteyner_id": container_id,
            "tail": tail,
        }))?;
        let response = self.read_json_line()?;
        if !is_ok(&response) {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&response, "Docker loglari alinamadi"),
            ));
        }

        let logs = response
            .get("loglar")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(logs)
    }

    /// Uzak agent üzerindeki Docker delillerini (tar.gz paketi olarak) akışla yerel dosyaya indirir.
    pub fn acquire_remote_docker<F>(
        &mut self,
        container_id: &str,
        acquire_diff: bool,
        acquire_logs: bool,
        acquire_config: bool,
        local_path: impl AsRef<Path>,
        job_id: Option<&str>,
        mut progress: F,
    ) -> AmeleResult<RemoteTransferResult>
    where
        F: FnMut(u64, u64),
    {
        if let Some(parent) = local_path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AmeleError::io(HataKodu::DosyaYazma, "Hedef klasor olusturulamadi", err)
            })?;
        }

        let mut request = json!({
            "komut": "docker_edinim_baslat",
            "konteyner_id": container_id,
            "acquire_diff": acquire_diff,
            "acquire_logs": acquire_logs,
            "acquire_config": acquire_config,
        });
        if let Some(job_id) = job_id {
            request["is_id"] = Value::String(job_id.to_string());
        }
        self.send_json(&request)?;

        let start = self.read_json_line()?;
        if !is_ok(&start) {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&start, "Docker edinimi baslatilamadi"),
            ));
        }

        let mut transfer_job_id = start
            .get("is_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut total = start
            .get("tahmini_boyut")
            .and_then(Value::as_u64)
            .unwrap_or_default();

        let data_start = self.read_json_line()?;
        if data_start.get("tur").and_then(Value::as_str) != Some("veri_basliyor") {
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&data_start, "Veri baslangici alinamadi"),
            ));
        }
        if transfer_job_id.is_empty() {
            transfer_job_id = data_start
                .get("is_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
        total = data_start
            .get("toplam")
            .and_then(Value::as_u64)
            .unwrap_or(total);

        let mut output = File::create(local_path.as_ref())
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Yerel dosya acilamadi", err))?;
        let mut transferred = 0_u64;
        let mut buffer = vec![0_u8; DEFAULT_CHUNK_SIZE];

        while transferred < total {
            let to_read = (total - transferred).min(buffer.len() as u64) as usize;
            let read = self
                .reader
                .read(&mut buffer[..to_read])
                .map_err(|err| AmeleError::io(HataKodu::AgAlma, "Uzak veri okunamadi", err))?;
            if read == 0 {
                drop(output);
                let _ = mark_partial(local_path.as_ref());
                return Err(AmeleError::new(
                    HataKodu::BaglantiKesildi,
                    "Ajan baglantisi kesildi",
                ));
            }
            output
                .write_all(&buffer[..read])
                .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Uzak veri yazilamadi", err))?;
            transferred += read as u64;
            progress(transferred, total);
        }
        output.flush().map_err(|err| {
            AmeleError::io(HataKodu::DosyaYazma, "Hedef dosya flush edilemedi", err)
        })?;
        drop(output);

        let end = self.read_json_line()?;
        if end.get("tur").and_then(Value::as_str) != Some("bitti") {
            let _ = mark_partial(local_path.as_ref());
            return Err(AmeleError::new(
                HataKodu::Protokol,
                message_from(&end, "Bitis mesaji alinamadi"),
            ));
        }

        Ok(RemoteTransferResult {
            job_id: transfer_job_id,
            target_path: local_path.as_ref().to_path_buf(),
            bytes_transferred: transferred,
            sha256: end
                .get("sha256")
                .and_then(Value::as_str)
                .map(str::to_string),
            md5: end.get("md5").and_then(Value::as_str).map(str::to_string),
            message: message_from(&end, "Docker delil paketi aktarildi"),
        })
    }

    /// Bağlantı açılışında agent kimliği, sürümü ve özelliklerini doğrular.
    fn hello(&mut self) -> AmeleResult<()> {
        let mut request = json!({
            "komut": "merhaba",
            "istemci": "amele",
            "surum": "0.1",
        });

        if let Some(token) = &self.token {
            request["token"] = Value::String(token.clone());
            request["guvenlik_anahtar_b64"] = Value::String(STANDARD.encode(token.as_bytes()));
        }

        self.send_json(&request)?;
        let response = self.read_json_line()?;
        if !is_ok(&response) {
            let message = message_from(&response, "Uzak uc beklenen ajan yaniti vermedi");
            self.last_error = message.clone();
            return Err(AmeleError::new(HataKodu::Guvenlik, message));
        }

        self.server_name = response
            .get("sunucu")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.server_version = response
            .get("surum")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.features = response
            .get("ozellikler")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        Ok(())
    }

    /// JSON komutunu satır sonuyla agent'a gönderir.
    fn send_json(&mut self, value: &Value) -> AmeleResult<()> {
        serde_json::to_writer(&mut self.writer, value)?;
        self.writer
            .write_all(b"\n")
            .map_err(|err| AmeleError::io(HataKodu::AgGonderme, "Komut gonderilemedi", err))?;
        self.writer
            .flush()
            .map_err(|err| AmeleError::io(HataKodu::AgGonderme, "Komut flush edilemedi", err))
    }

    /// Agent'tan tek satır JSON cevap okur.
    fn read_json_line(&mut self) -> AmeleResult<Value> {
        let mut line = String::new();
        let read = self
            .reader
            .read_line(&mut line)
            .map_err(|err| AmeleError::io(HataKodu::AgAlma, "Yanit alinamadi", err))?;
        if read == 0 {
            return Err(AmeleError::new(
                HataKodu::BaglantiKesildi,
                "Baglanti kapandi",
            ));
        }
        serde_json::from_str(line.trim_end()).map_err(|err| {
            AmeleError::new(
                HataKodu::ProtokolJson,
                format!("Gecersiz JSON yaniti: {err}"),
            )
        })
    }
}

/// Araç kontrol cevabını ortak RemoteToolStatus modeline dönüştürür.
fn parse_tool_status(
    response: &Value,
    present_key: &str,
    path_key: &str,
) -> AmeleResult<RemoteToolStatus> {
    if !is_ok(response) {
        return Err(AmeleError::new(
            HataKodu::Protokol,
            message_from(response, "Ajan kontrol hatasi"),
        ));
    }

    Ok(RemoteToolStatus {
        tool_present: response
            .get(present_key)
            .and_then(Value::as_bool)
            .unwrap_or(false),
        admin_privilege: response
            .get("yonetici_yetkisi")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ram_size: response
            .get("ram_boyut")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        tool_path: response
            .get(path_key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        message: message_from(response, ""),
    })
}

/// Agent cevabının başarılı durum taşıyıp taşımadığını kontrol eder.
fn is_ok(value: &Value) -> bool {
    value.get("durum").and_then(Value::as_str) == Some("ok")
}

/// Agent cevabının hata durum taşıyıp taşımadığını kontrol eder.
fn is_error(value: &Value) -> bool {
    value.get("durum").and_then(Value::as_str) == Some("hata")
}

/// Agent mesaj alanını güvenli varsayılanla okur.
fn message_from(value: &Value, default: &str) -> String {
    value
        .get("mesaj")
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

/// Dosya adında kullanılacak IP/disk değerlerini güvenli karakterlere indirger.
fn sanitize_name(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Uzak imaj çıktısı için IP, disk ve tarih içeren standart dosya adı üretir.
fn canonical_image_file_name(
    remote_ip: Option<&str>,
    disk_id: &str,
    disk_name: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(ip) = remote_ip
        .map(sanitize_name)
        .filter(|value| !value.is_empty())
    {
        parts.push(ip);
    }

    let disk_id = sanitize_name(disk_id);
    parts.push(if disk_id.is_empty() {
        "disk".to_string()
    } else {
        disk_id
    });

    if let Some(name) = disk_name
        .map(sanitize_name)
        .filter(|value| !value.is_empty())
        && parts.last().map(|last| last != &name).unwrap_or(true)
    {
        parts.push(name);
    }

    format!(
        "{}_{}.img",
        parts.join("_"),
        Local::now().format("%Y%m%d_%H%M%S")
    )
}

/// Eksik aktarım dosyasını .partial adıyla korur.
fn mark_partial(path: &Path) -> AmeleResult<PathBuf> {
    let partial = PathBuf::from(format!("{}.partial", path.display()));
    if path.exists() {
        fs::rename(path, &partial)
            .map_err(|err| AmeleError::io(HataKodu::DosyaYazma, "Partial dosya tasinamadi", err))?;
    }
    Ok(partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn lists_disks_with_agent_protocol() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                stream,
                "{}",
                json!({"durum":"ok","sunucu":"linux-ajan","surum":"0.1","ozellikler":["disk_imaj"]})
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("disk_listele"));
            writeln!(
                stream,
                "{}",
                json!({"durum":"ok","diskler":[{"id":"sda","ad":"sda","boyut":1234}]})
            )
            .unwrap();
        });

        let mut conn = RemoteConnection::connect("127.0.0.1", port, None).unwrap();
        let disks = conn.list_disks().unwrap();
        assert_eq!(disks[0].id, "sda");
        assert_eq!(disks[0].boyut, 1234);
    }

    #[test]
    fn acquires_image_stream_with_agent_protocol() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                stream,
                "{}",
                json!({"durum":"ok","sunucu":"windows-ajan","surum":"0.4","ozellikler":["disk_imaj"]})
            )
            .unwrap();

            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("imaj_baslat"));
            writeln!(
                stream,
                "{}",
                json!({"durum":"ok","is_id":"IMG_TEST","tahmini_boyut":9})
            )
            .unwrap();
            writeln!(
                stream,
                "{}",
                json!({"tur":"veri_basliyor","is_id":"IMG_TEST","toplam":9})
            )
            .unwrap();
            stream.write_all(b"disk-data").unwrap();
            writeln!(
                stream,
                "{}",
                json!({"tur":"bitti","is_id":"IMG_TEST","sha256":"hash","md5":"md5"})
            )
            .unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        let mut conn = RemoteConnection::connect("127.0.0.1", port, None).unwrap();
        let result = conn
            .acquire_image(
                "0",
                None,
                dir.path(),
                None,
                AcquisitionOutputFormat::Raw,
                |_done, _total| {},
            )
            .unwrap();
        assert_eq!(result.job_id, "IMG_TEST");
        assert_eq!(std::fs::read(result.target_path).unwrap(), b"disk-data");
        assert_eq!(result.sha256.as_deref(), Some("hash"));
    }

    fn bind_test_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => Some(listener),
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => None,
            Err(err) => panic!("test listener bind failed: {err}"),
        }
    }
}
