//! Android dosya sistemi edinimini root veya ADB tabanlı yöntemlerle yürütür.
use super::adb::run_adb_command;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
/// Android dosya sistemi edinimi sonunda üretilen imaj/arşiv bilgisini taşır.
pub struct FilesystemAcquisitionResult {
    pub output_file: std::path::PathBuf,
    pub total_bytes: u64,
    pub sha256: String,
}

/// Root erişimi varsa /data blok imajı, yoksa tar arşivi üretmeye çalışan edinim akışıdır.
pub fn filesystem_acquisition<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    has_root: bool,
    mut progress: F,
    cancelled: C,
) -> Result<FilesystemAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    std::fs::create_dir_all(output_dir)
        .map_err(|err| format!("Cikti dizini olusturulamadi: {err}"))?;

    // İlk adımda root yetkisi doğrulanır veya adb root denenir.
    progress(0, 3, "Root yetkisi kontrol ediliyor...");

    let mut is_root = false;
    let mut use_su = false;

    // Mevcut shell oturumunun root olup olmadığı kontrol edilir.
    if let Ok(out) = run_adb_command(serial, &["shell", "id"]) {
        if out.contains("uid=0(root)") || out.contains("root") {
            is_root = true;
        }
    }

    if !is_root {
        if let Ok(out) = run_adb_command(serial, &["shell", "su", "-c", "id"]) {
            if out.contains("uid=0(root)") || out.contains("root") {
                is_root = true;
                use_su = true;
            }
        }
    }

    // Kullanıcı root var demediyse adb root ile yükseltme denenir.
    if !is_root && !has_root {
        progress(0, 3, "Root yetkisi bulunamadi, 'adb root' deneniyor...");
        let _ = Command::new("adb").args(["-s", serial, "root"]).output();
        std::thread::sleep(std::time::Duration::from_secs(3));

        // adb root sonrası yetki durumu yeniden kontrol edilir.
        if let Ok(out) = run_adb_command(serial, &["shell", "id"]) {
            if out.contains("uid=0(root)") || out.contains("root") {
                is_root = true;
            }
        }

        if !is_root {
            if let Ok(out) = run_adb_command(serial, &["shell", "su", "-c", "id"]) {
                if out.contains("uid=0(root)") || out.contains("root") {
                    is_root = true;
                    use_su = true;
                }
            }
        }
    }

    if !is_root {
        if has_root {
            return Err("Cihazda root yetkisi alinamadi. Root dosya sistemi imaji icin su/adbd root dogrulanmali.".to_string());
        }
        return non_root_filesystem_acquisition(serial, output_dir, progress, cancelled);
    }

    progress(
        1,
        3,
        "Root yetkisi doğrulandı. Bölüm bilgileri analiz ediliyor...",
    );

    // /data bölümünün blok aygıtı bulunursa ham imaj alınır.
    let mut block_device = None;
    let mount_cmd = if use_su {
        "su -c 'cat /proc/mounts'"
    } else {
        "cat /proc/mounts"
    };

    if let Ok(out) = run_adb_command(serial, &["shell", mount_cmd]) {
        for line in out.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "/data" {
                block_device = Some(parts[0].to_string());
                break;
            }
        }
    }

    let output_file_name = if block_device.is_some() {
        "userdata.img"
    } else {
        "filesystem.tar"
    };

    let output_path = output_dir.join(output_file_name);
    let mut file = std::fs::File::create(&output_path)
        .map_err(|err| format!("Hedef dosya olusturulamadi: {err}"))?;

    let mut cmd = Command::new("adb");
    cmd.args(["-s", serial]);

    if let Some(ref dev) = block_device {
        let status_msg = format!("Bölüm aygıtı bulundu: {dev}. Disk imajı (dd) aktarılıyor...");
        progress(1, 3, &status_msg);

        if use_su {
            cmd.args([
                "exec-out",
                &format!("su -c 'dd if={} bs=4096 2>/dev/null'", dev),
            ]);
        } else {
            cmd.args(["exec-out", &format!("dd if={} bs=4096 2>/dev/null", dev)]);
        }
    } else {
        progress(
            1,
            3,
            "Bölüm aygıtı bulunamadı. Dosya arşivi (tar) aktarılıyor...",
        );

        if use_su {
            cmd.args(["exec-out", "su -c 'tar -cf - /data 2>/dev/null'"]);
        } else {
            cmd.args(["exec-out", "tar -cf - /data 2>/dev/null"]);
        }
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("ADB baslatilamadi: {err}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ADB cikti akisi alinamadi".to_string())?;

    let mut buffer = [0; 65536];
    let mut total_bytes = 0_u64;
    let mut last_progress_bytes = 0_u64;

    loop {
        if cancelled() {
            let _ = child.kill();
            return Err("Kullanici tarafindan iptal edildi".to_string());
        }

        let bytes_read = stdout
            .read(&mut buffer)
            .map_err(|err| format!("Cihazdan veri okunurken hata olustu: {err}"))?;

        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .map_err(|err| format!("Dosyaya veri yazilirken hata olustu: {err}"))?;

        total_bytes += bytes_read as u64;

        if total_bytes - last_progress_bytes > 10 * 1024 * 1024 {
            last_progress_bytes = total_bytes;
            let mb = total_bytes / (1024 * 1024);
            let mode_str = if block_device.is_some() {
                "Disk İmajı"
            } else {
                "Dosya Arşivi"
            };
            progress(1, 3, &format!("{} aktarılıyor: {} MB", mode_str, mb));
        }
    }

    let _ = child.wait();

    if total_bytes == 0 {
        return Err("Aktarilan veri bos (0 byte). Gecersiz imaj.".to_string());
    }

    // Çıktı bütünlüğü için SHA-256 yan dosyası üretilir.
    progress(
        2,
        3,
        "İmaj aktarımı tamamlandı, bütünlük doğrulama özeti (SHA-256) hesaplanıyor...",
    );

    let mut file = std::fs::File::open(&output_path)
        .map_err(|err| format!("Olusturulan imaj dosyasi acilamadi: {err}"))?;

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();

    let mut hashed_bytes = 0_u64;
    let mut last_hash_progress_bytes = 0_u64;

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|err| format!("Hash hesabi icin dosya okunurken hata: {err}"))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        hashed_bytes += bytes_read as u64;

        if hashed_bytes - last_hash_progress_bytes > 50 * 1024 * 1024 {
            last_hash_progress_bytes = hashed_bytes;
            let mb = hashed_bytes / (1024 * 1024);
            progress(
                2,
                3,
                &format!("Doğrulama özeti (SHA-256) hesaplanıyor: {} MB", mb),
            );
        }
    }

    let sha256 = crate::hash::to_hex(&hasher.finalize());

    // SHA-256 sonucu aynı klasöre sidecar dosyası olarak yazılır.
    let sidecar_path = output_dir.join(format!("{}.sha256", output_file_name));
    let _ = std::fs::write(&sidecar_path, format!("{sha256}  {}\n", output_file_name));

    progress(3, 3, "İşlem başarıyla tamamlandı!");

    Ok(FilesystemAcquisitionResult {
        output_file: output_path,
        total_bytes,
        sha256,
    })
}

/// Root yokken erişilebilir Android dosyalarını tek arşivde toplar.
fn non_root_filesystem_acquisition<F, C>(
    serial: &str,
    output_dir: &Path,
    mut progress: F,
    cancelled: C,
) -> Result<FilesystemAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    progress(
        1,
        4,
        "Root yok. Non-root paylaşımlı depolama ve dosya indeksi toplanıyor...",
    );

    let staging_dir = output_dir.join("android_nonroot_filesystem_staging");
    if staging_dir.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
    }
    std::fs::create_dir_all(&staging_dir)
        .map_err(|err| format!("Non-root gecici klasor olusturulamadi: {err}"))?;

    let sdcard_dir = staging_dir.join("sdcard");
    std::fs::create_dir_all(&sdcard_dir)
        .map_err(|err| format!("Paylasimli depolama klasoru olusturulamadi: {err}"))?;
    pull_shared_storage(serial, &sdcard_dir, &cancelled)?;

    if cancelled() {
        return Err("Kullanici tarafindan iptal edildi".to_string());
    }

    progress(
        2,
        4,
        "Non-root dosya indeksi ve mount bilgisi kaydediliyor...",
    );
    write_non_root_metadata(serial, &staging_dir);

    if cancelled() {
        return Err("Kullanici tarafindan iptal edildi".to_string());
    }

    progress(3, 4, "Non-root Android dosya arşivi oluşturuluyor...");
    let output_file_name = "android_nonroot_filesystem.tar";
    let output_path = output_dir.join(output_file_name);
    let file = std::fs::File::create(&output_path)
        .map_err(|err| format!("Non-root arşiv dosyasi olusturulamadi: {err}"))?;
    let mut archive = tar::Builder::new(file);
    archive
        .append_dir_all("android_nonroot_filesystem", &staging_dir)
        .map_err(|err| format!("Non-root arşiv olusturulamadi: {err}"))?;
    archive
        .finish()
        .map_err(|err| format!("Non-root arşiv kapatilamadi: {err}"))?;

    let _ = std::fs::remove_dir_all(&staging_dir);

    let total_bytes = std::fs::metadata(&output_path)
        .map_err(|err| format!("Non-root arşiv metaverisi okunamadi: {err}"))?
        .len();
    if total_bytes == 0 {
        return Err("Non-root Android dosya arşivi bos olustu.".to_string());
    }

    progress(3, 4, "Non-root arşiv SHA-256 özeti hesaplanıyor...");
    let sha256 = hash_file_with_progress(&output_path, |mb| {
        progress(
            3,
            4,
            &format!("Non-root arşiv SHA-256 hesaplanıyor: {mb} MB"),
        )
    })?;

    let sidecar_path = output_dir.join(format!("{}.sha256", output_file_name));
    let _ = std::fs::write(&sidecar_path, format!("{sha256}  {output_file_name}\n"));

    progress(4, 4, "Non-root Android dosya arşivi tamamlandı.");

    Ok(FilesystemAcquisitionResult {
        output_file: output_path,
        total_bytes,
        sha256,
    })
}

/// /sdcard içeriğini ADB pull ile yerel geçici klasöre indirir.
fn pull_shared_storage<C>(serial: &str, target_dir: &Path, cancelled: &C) -> Result<(), String>
where
    C: Fn() -> bool,
{
    let target = target_dir.to_string_lossy().into_owned();
    let mut child = Command::new("adb")
        .args(["-s", serial, "pull", "/sdcard/", &target])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("ADB pull baslatilamadi: {err}"))?;

    loop {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Kullanici tarafindan iptal edildi".to_string());
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|err| format!("ADB pull sonucu alinamadi: {err}"))?;
                if !output.status.success() && dir_size(target_dir) == 0 {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("Paylasimli depolama indirilemedi: {stderr}"));
                }
                return Ok(());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(250)),
            Err(err) => return Err(format!("ADB pull durumu okunamadi: {err}")),
        }
    }
}

/// Non-root arşive eklenecek dosya indeksi ve sistem özetlerini yazar.
fn write_non_root_metadata(serial: &str, staging_dir: &Path) {
    let file_index = run_adb_command(
        serial,
        &[
            "shell",
            "find /sdcard -maxdepth 5 -type f 2>/dev/null | head -n 20000",
        ],
    )
    .unwrap_or_else(|err| format!("file_index_error={err}\n"));
    let _ = std::fs::write(staging_dir.join("file_index.txt"), file_index);

    let mounts = run_adb_command(
        serial,
        &[
            "shell",
            "cat /proc/mounts; echo '=== df ==='; df -h; echo '=== storage ==='; ls -la /storage 2>/dev/null || true",
        ],
    )
    .unwrap_or_else(|err| format!("mounts_error={err}\n"));
    let _ = std::fs::write(staging_dir.join("mounts_and_storage.txt"), mounts);
}

/// Dosyanın SHA-256 değerini hesaplarken MB ilerlemesini bildirir.
fn hash_file_with_progress<F>(path: &Path, mut progress: F) -> Result<String, String>
where
    F: FnMut(u64),
{
    use sha2::{Digest, Sha256};

    let mut file =
        std::fs::File::open(path).map_err(|err| format!("Hash icin dosya acilamadi: {err}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut hashed_bytes = 0_u64;
    let mut last_hash_progress_bytes = 0_u64;

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|err| format!("Hash icin dosya okunamadi: {err}"))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        hashed_bytes += bytes_read as u64;

        if hashed_bytes - last_hash_progress_bytes > 50 * 1024 * 1024 {
            last_hash_progress_bytes = hashed_bytes;
            progress(hashed_bytes / (1024 * 1024));
        }
    }

    Ok(crate::hash::to_hex(&hasher.finalize()))
}

/// Klasör içindeki dosyaların toplam boyutunu hesaplar.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0_u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else {
                total += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}
