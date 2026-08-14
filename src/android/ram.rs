//! Android uçucu veri ve root destekli süreç belleği toplama akışlarını yürütür.
use super::adb::run_adb_command;
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Android RAM ediniminde seçilen çalışma modunu temsil eder.
pub enum AndroidRamMode {
    VolatileData,
    RootProcessMemory,
    PhysicalMemoryProbe,
}

impl AndroidRamMode {
    /// UI'den gelen mod kimliğini güvenli varsayılanla enum değerine çevirir.
    pub fn from_id(value: &str) -> Self {
        match value.trim() {
            "root_process_memory" | "root" => Self::RootProcessMemory,
            "physical_memory_probe" | "physical" => Self::PhysicalMemoryProbe,
            _ => Self::VolatileData,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
/// Android RAM edinimi sonunda üretilen dosya, boyut ve hash bilgisini taşır.
pub struct AndroidRamAcquisitionResult {
    pub output_file: std::path::PathBuf,
    pub total_bytes: u64,
    pub sha256: String,
    pub mode: AndroidRamMode,
}

/// Varsayılan fiziksel bellek deneme moduyla Android RAM edinimini başlatır.
pub fn ram_acquisition<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    has_root: bool,
    progress: F,
    cancelled: C,
) -> Result<AndroidRamAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    ram_acquisition_with_mode(
        serial,
        output_dir,
        has_root,
        AndroidRamMode::PhysicalMemoryProbe,
        progress,
        cancelled,
    )
}

/// Seçilen moda göre fiziksel bellek, proses belleği veya uçucu veri toplar.
pub fn ram_acquisition_with_mode<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    has_root: bool,
    mode: AndroidRamMode,
    mut progress: F,
    cancelled: C,
) -> Result<AndroidRamAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    std::fs::create_dir_all(output_dir)
        .map_err(|err| format!("Cikti dizini olusturulamadi: {err}"))?;

    if mode == AndroidRamMode::VolatileData {
        return collect_volatile_data(serial, output_dir, progress, cancelled);
    }

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
        return Err("Cihazda root yetkisi alinamadi. Bellek (RAM) imaji ancak root yetkisine sahip cihazlarda alinabilir.".to_string());
    }

    progress(
        1,
        3,
        "Root yetkisi doğrulandı. Bellek kaynakları analiz ediliyor...",
    );

    // Fiziksel bellek modu seçildiyse okunabilir bellek arayüzü aranır.
    let mut memory_source = None;
    let check_cmd = |path: &str| -> bool {
        let cmd = if use_su {
            format!("su -c 'test -r {} && echo OK'", path)
        } else {
            format!("test -r {} && echo OK", path)
        };
        run_adb_command(serial, &["shell", &cmd])
            .map(|out| out.trim() == "OK")
            .unwrap_or(false)
    };

    if mode == AndroidRamMode::PhysicalMemoryProbe {
        if check_cmd("/proc/kcore") {
            memory_source = Some("/proc/kcore");
        } else if check_cmd("/dev/mem") {
            memory_source = Some("/dev/mem");
        } else if check_cmd("/dev/kmem") {
            memory_source = Some("/dev/kmem");
        }
    }

    let source = match memory_source {
        Some(src) => src,
        None => {
            progress(
                1,
                3,
                "Fiziksel bellek arayüzleri kısıtlı. Canlı uçucu bellek (Logical Process RAM) moduna otomatik geçiş yapılıyor...",
            );

            // Fiziksel bellek kapalıysa proses odaklı mantıksal döküme geçilir.
            let script_path = output_dir.join("memdump.sh");
            let script_content = r#"#!/system/bin/sh
# AMELE Forensic Suite - Logical RAM & Volatile Memory Dumper
OUT_DIR="/data/local/tmp/amele_ram_dumps"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "[AMELE] Uçucu bellek analizi başlatıldı..."

# List of interesting process names or package patterns
ps -A -o PID,NAME | while read -r pid name; do
  [ "$pid" = "PID" ] && continue
  [ -z "$pid" ] && continue
  
  if [ -f "/proc/$pid/maps" ] && [ -f "/proc/$pid/mem" ]; then
    is_interesting=0
    case "$name" in
      *.*|system_server|init|zygote*|*chrome*|*browser*|*webview*|*telegram*|*whatsapp*|*signal*|*discord*)
        is_interesting=1
        ;;
    esac
    
    [ $is_interesting -eq 0 ] && continue
    
    echo "[AMELE_PROGRESS] Dumping: $name ($pid)"
    PID_DIR="$OUT_DIR/$pid"
    mkdir -p "$PID_DIR"
    echo "$name" > "$PID_DIR/name.txt"
    cat "/proc/$pid/maps" > "$PID_DIR/maps"
    
    cat "/proc/$pid/maps" | grep -E "r-p|rw-p" | while read -r start_end perm offset dev inode path; do
      case "$path" in
        /system/*|/vendor/*|/apex/*|/system_ext/*|/data/app/*/*.so|/data/app/*/*.apk|*.so|*.apk|*.ttf|*.otf|*font*)
          continue
          ;;
      esac
      
      start=$(echo "$start_end" | cut -d'-' -f1)
      end=$(echo "$start_end" | cut -d'-' -f2)
      
      start_dec=$((0x$start))
      end_dec=$((0x$end))
      size=$((end_dec - start_dec))
      
      if [ $size -gt 0 ]; then
        dd if="/proc/$pid/mem" of="$PID_DIR/${start}_${end}.bin" bs=4096 skip=$((start_dec / 4096)) count=$((size / 4096)) 2>/dev/null
      fi
    done
  fi
done

echo "[AMELE] Uçucu bellekler paketleniyor..."
cd /data/local/tmp
tar -cf amele_ram_dumps.tar amele_ram_dumps
rm -rf amele_ram_dumps
echo "[AMELE] DONE"
"#;

            std::fs::write(&script_path, script_content)
                .map_err(|err| format!("Umut yerel betik olusturulamadi: {err}"))?;

            // Hazırlanan betik cihaza geçici klasöre gönderilir.
            progress(1, 3, "Uçucu bellek analiz betiği cihaza gönderiliyor...");
            let push_out = Command::new("adb")
                .args([
                    "-s",
                    serial,
                    "push",
                    script_path
                        .to_str()
                        .ok_or_else(|| "Geçersiz betik dosya yolu".to_string())?,
                    "/data/local/tmp/memdump.sh",
                ])
                .output()
                .map_err(|err| format!("Betiği cihaza push etme başarısız: {err}"))?;

            if !push_out.status.success() {
                return Err(format!(
                    "Betiği cihaza yükleme başarısız: {}",
                    String::from_utf8_lossy(&push_out.stderr)
                ));
            }

            // Yerel geçici betik temizlenir.
            let _ = std::fs::remove_file(&script_path);

            // Betik çalıştırılabilir yapılır ve root yetkisiyle başlatılır.
            progress(1, 3, "Uçucu bellek analiz betiği çalıştırılıyor...");
            let chmod_cmd = if use_su {
                "su -c 'chmod 755 /data/local/tmp/memdump.sh'"
            } else {
                "chmod 755 /data/local/tmp/memdump.sh"
            };
            let _ = Command::new("adb")
                .args(["-s", serial, "shell", chmod_cmd])
                .output();

            let run_cmd = if use_su {
                "su -c '/data/local/tmp/memdump.sh'"
            } else {
                "/data/local/tmp/memdump.sh"
            };

            let mut run_proc = Command::new("adb")
                .args(["-s", serial, "shell", run_cmd])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|err| format!("Uçucu bellek betiği başlatılamadı: {err}"))?;

            let run_stdout = run_proc
                .stdout
                .take()
                .ok_or_else(|| "Betiğin çıktı akışı alınamadı".to_string())?;
            let reader = std::io::BufReader::new(run_stdout);

            use std::io::BufRead;
            for line in reader.lines() {
                if cancelled() {
                    let _ = run_proc.kill();
                    return Err("Kullanici tarafindan iptal edildi".to_string());
                }
                if let Ok(l) = line {
                    if l.starts_with("[AMELE_PROGRESS]") {
                        let proc_info = l.trim_start_matches("[AMELE_PROGRESS] ").trim();
                        progress(1, 3, &format!("RAM imajı aktarılıyor: {}", proc_info));
                    }
                }
            }

            let _ = run_proc.wait();

            // Cihazda üretilen tar arşivi vaka klasörüne çekilir.
            progress(
                2,
                3,
                "Toplanan uçucu bellek verileri cihaza indiriliyor (tar arşivi)...",
            );
            let output_file_name = "android_logical_ram.tar";
            let output_path = output_dir.join(output_file_name);

            let pull_out = Command::new("adb")
                .args([
                    "-s",
                    serial,
                    "pull",
                    "/data/local/tmp/amele_ram_dumps.tar",
                    output_path
                        .to_str()
                        .ok_or_else(|| "Geçersiz çıktı dosya yolu".to_string())?,
                ])
                .output()
                .map_err(|err| format!("Bellek paketini çekme başarısız: {err}"))?;

            if !pull_out.status.success() {
                return Err(format!(
                    "Bellek paketini çekme başarısız: {}",
                    String::from_utf8_lossy(&pull_out.stderr)
                ));
            }

            // Cihazdaki geçici betik ve arşiv temizlenir.
            let rm_cmd = if use_su {
                "su -c 'rm -f /data/local/tmp/memdump.sh /data/local/tmp/amele_ram_dumps.tar'"
            } else {
                "rm -f /data/local/tmp/memdump.sh /data/local/tmp/amele_ram_dumps.tar"
            };
            let _ = Command::new("adb")
                .args(["-s", serial, "shell", rm_cmd])
                .output();

            let metadata = std::fs::metadata(&output_path)
                .map_err(|err| format!("Bellek dosyası metaverisi okunamadı: {err}"))?;
            let total_bytes = metadata.len();

            // Çıktı bütünlüğü için SHA-256 yan dosyası üretilir.
            progress(
                2,
                3,
                "RAM imajı tamamlandı, bütünlük doğrulama özeti (SHA-256) hesaplanıyor...",
            );

            let mut file = std::fs::File::open(&output_path)
                .map_err(|err| format!("Olusturulan RAM imaj dosyasi acilamadi: {err}"))?;

            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            let mut buffer = [0; 65536];
            let mut hashed_bytes = 0_u64;
            let mut last_hash_progress_bytes = 0_u64;

            loop {
                if cancelled() {
                    return Err("Kullanici tarafindan iptal edildi".to_string());
                }
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

            return Ok(AndroidRamAcquisitionResult {
                output_file: output_path,
                total_bytes,
                sha256,
                mode,
            });
        }
    };

    let output_file_name = "android_ram.bin";
    let output_path = output_dir.join(output_file_name);
    let mut file = std::fs::File::create(&output_path)
        .map_err(|err| format!("Hedef dosya olusturulamadi: {err}"))?;

    let mut cmd = Command::new("adb");
    cmd.args(["-s", serial]);

    let status_msg = format!("Bellek kaynağı bulundu: {source}. RAM imajı aktarılıyor...");
    progress(1, 3, &status_msg);

    if use_su {
        cmd.args([
            "exec-out",
            &format!("su -c 'dd if={} bs=4096 2>/dev/null'", source),
        ]);
    } else {
        cmd.args(["exec-out", &format!("dd if={} bs=4096 2>/dev/null", source)]);
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

    use std::io::{Read, Write};

    loop {
        if cancelled() {
            let _ = child.kill();
            return Err("Kullanici tarafindan iptal edildi".to_string());
        }

        let bytes_read = stdout
            .read(&mut buffer)
            .map_err(|err| format!("Cihazdan bellek verisi okunurken hata olustu: {err}"))?;

        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .map_err(|err| format!("Dosyaya bellek verisi yazilirken hata olustu: {err}"))?;

        total_bytes += bytes_read as u64;

        if total_bytes - last_progress_bytes > 10 * 1024 * 1024 {
            last_progress_bytes = total_bytes;
            let mb = total_bytes / (1024 * 1024);
            progress(1, 3, &format!("RAM imajı aktarılıyor: {} MB", mb));
        }
    }

    let _ = child.wait();

    if total_bytes == 0 {
        return Err("Aktarilan bellek verisi bos (0 byte). RAM imaji basarisiz.".to_string());
    }

    // Fiziksel RAM çıktısı için SHA-256 bütünlük özeti hesaplanır.
    progress(
        2,
        3,
        "RAM imajı tamamlandı, bütünlük doğrulama özeti (SHA-256) hesaplanıyor...",
    );

    let mut file = std::fs::File::open(&output_path)
        .map_err(|err| format!("Olusturulan RAM imaj dosyasi acilamadi: {err}"))?;

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

    Ok(AndroidRamAcquisitionResult {
        output_file: output_path,
        total_bytes,
        sha256,
        mode,
    })
}

/// Root gerektirmeyen uçucu Android bilgilerini tek metin dosyasında toplar.
fn collect_volatile_data<F, C>(
    serial: &str,
    output_dir: &std::path::Path,
    mut progress: F,
    cancelled: C,
) -> Result<AndroidRamAcquisitionResult, String>
where
    F: FnMut(u32, u32, &str),
    C: Fn() -> bool,
{
    let output_file_name = "android_volatile_data.txt";
    let output_path = output_dir.join(output_file_name);
    let commands = [
        ("meminfo", "cat /proc/meminfo"),
        ("vmstat", "cat /proc/vmstat"),
        ("uptime", "cat /proc/uptime"),
        ("processes", "ps -A"),
        ("dumpsys_meminfo", "dumpsys meminfo"),
        ("dumpsys_procstats", "dumpsys procstats"),
        ("activity_processes", "dumpsys activity processes"),
        ("logcat_tail", "logcat -d | tail -n 1000"),
    ];

    let total = commands.len() as u32 + 1;
    let mut content = String::new();
    content.push_str("# Android volatile data acquisition\n\n");

    for (index, (label, command)) in commands.iter().enumerate() {
        if cancelled() {
            return Err("Kullanici tarafindan iptal edildi".to_string());
        }
        progress(index as u32, total, label);
        content.push_str("=== ");
        content.push_str(label);
        content.push_str(" ===\n");
        match run_adb_command(serial, &["shell", command]) {
            Ok(output) => content.push_str(&output),
            Err(err) => {
                content.push_str("ERROR: ");
                content.push_str(&err);
                content.push('\n');
            }
        }
        content.push('\n');
    }

    std::fs::write(&output_path, content)
        .map_err(|err| format!("Uçucu veri özeti yazılamadı: {err}"))?;

    progress(total - 1, total, "sha256");
    let mut file = std::fs::File::open(&output_path)
        .map_err(|err| format!("Uçucu veri dosyasi acilamadi: {err}"))?;
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|err| format!("Hash hesabi icin dosya okunurken hata: {err}"))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    let sha256 = crate::hash::to_hex(&hasher.finalize());
    let _ = std::fs::write(
        output_dir.join(format!("{output_file_name}.sha256")),
        format!("{sha256}  {output_file_name}\n"),
    );
    let total_bytes = std::fs::metadata(&output_path)
        .map_err(|err| format!("Uçucu veri dosyası metaverisi okunamadı: {err}"))?
        .len();

    progress(total, total, "done");
    Ok(AndroidRamAcquisitionResult {
        output_file: output_path,
        total_bytes,
        sha256,
        mode: AndroidRamMode::VolatileData,
    })
}
