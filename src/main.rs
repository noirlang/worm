//! Komut satırı girişini işler ve UI, browser veya helper modlarını başlatır.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
use amele::android;
use amele::android_analysis;
use amele::api;
use amele::disk;
use amele::disk_analysis;
use amele::docker::{self, DockerAcquisitionRequest};
use amele::evidence::EvidenceVault;
use amele::hash::{self, HashAlgorithm};
use amele::ios;
use amele::output_format::{self, AcquisitionOutputFormat};
use amele::ram;
use amele::ram_analysis;
use amele::remote::RemoteConnection;
use amele::server;
use amele::settings::AppSettings;
use amele::wireguard::{self, WireGuardConfig};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

static IS_ENGLISH: AtomicBool = AtomicBool::new(false);

fn set_cli_english(val: bool) {
    IS_ENGLISH.store(val, Ordering::SeqCst);
}

fn is_cli_english() -> bool {
    IS_ENGLISH.load(Ordering::SeqCst)
}

fn t_cli(tr: &str, en: &str) -> String {
    if is_cli_english() {
        en.to_string()
    } else {
        tr.to_string()
    }
}

/// CLI argümanını okuyup ilgili alt komutu veya UI modunu çalıştırır.
fn main() {
    install_error_reporting();

    let mut raw_args: Vec<String> = std::env::args().skip(1).collect();
    let is_en = extract_global_en_flag(&mut raw_args);
    if is_en {
        set_cli_english(true);
    }
    let profile_arg = extract_global_profile(&mut raw_args);
    if let Some(username) = profile_arg {
        if let Err(err) = amele::profile::select_profile(&username, false) {
            report_fatal_error(&err.to_string());
            eprintln!("{err}");
            std::process::exit(2);
        }
    } else {
        let _ = amele::profile::bootstrap_profiles();
    }

    let mut args = raw_args.into_iter();
    let result = match args.next().as_deref() {
        Some("settings-default") => print_default_settings(),
        Some("profiles") | Some("profile-list") => profile_list_command(),
        Some("profile-create") => profile_create_command(args.collect()),
        Some("profile-use") | Some("profile-select") => profile_use_command(args.collect()),
        Some("profile-logout") => profile_logout_command(),
        Some("profile-online-sync") | Some("online-sync") => profile_online_sync_command(),
        Some("hash") => hash_command(args.collect()),
        Some("disk-list") => disk_list_command(),
        Some("local-image") => local_image_command(args.collect()),
        Some("local-ram") => local_ram_command(args.collect()),
        Some("remote-ram") => remote_ram_command(args.collect()),
        Some("image-analyze") => image_analyze_command(args.collect()),
        Some("ram-summary") => ram_summary_command(args.collect()),
        Some("ram-strings") => ram_strings_command(args.collect()),
        Some("ram-carve") => ram_carve_command(args.collect()),
        Some("ram-processes") => ram_processes_command(args.collect()),
        Some("adb-status") | Some("android-adb-status") => android_adb_status_command(),
        Some("adb-install") | Some("android-adb-install") => android_adb_install_command(),
        Some("android-devices") => android_devices_command(),
        Some("android-profile") => android_profile_command(args.collect()),
        Some("android-logical") => android_logical_command(args.collect()),
        Some("android-filesystem") => android_filesystem_command(args.collect()),
        Some("android-ram") => android_ram_command(args.collect()),
        Some("android-capabilities") => android_capabilities_command(args.collect()),
        Some("android-lemon-preflight") => android_lemon_preflight_command(args.collect()),
        Some("android-remote-connect") => android_remote_connect_command(args.collect()),
        Some("android-remote-disconnect") => android_remote_disconnect_command(args.collect()),
        Some("android-case-analysis") => android_case_analysis_command(args.collect()),
        Some("ios-backup-profile") => ios_backup_profile_command(args.collect()),
        Some("ios-backup-normalize") => ios_backup_normalize_command(args.collect()),
        Some("docker-status") => docker_status_command(args.collect()),
        Some("docker-list") | Some("docker-containers") => docker_list_command(args.collect()),
        Some("docker-logs") => docker_logs_command(args.collect()),
        Some("docker-acquire") => docker_acquire_command(args.collect()),
        Some("docker-remote-status") => docker_remote_status_command(args.collect()),
        Some("docker-remote-list") => docker_remote_list_command(args.collect()),
        Some("docker-remote-logs") => docker_remote_logs_command(args.collect()),
        Some("docker-remote-acquire") => docker_remote_acquire_command(args.collect()),
        Some("disk-list-helper") => disk_list_helper_command(args.collect()),
        Some("image-helper") => image_helper_command(args.collect()),
        Some("ram-helper") => ram_helper_command(args.collect()),
        Some("avml-install-helper") => avml_install_helper_command(args.collect()),
        Some("winpmem-install-helper") => winpmem_install_helper_command(args.collect()),
        Some("mount-helper") => mount_helper_command(args.collect()),
        Some("disk-size") => disk_size_command(args.collect()),
        Some("verify") => verify_command(args.collect()),
        Some("remote-disks") => remote_disks_command(args.collect()),
        Some("remote-image") => remote_image_command(args.collect()),
        Some("remote-tool-check") => remote_tool_check_command(args.collect()),
        Some("ram-status") => ram_status_command(),
        Some("wireguard-config") => wireguard_config_command(args.collect()),
        Some("update-check") | Some("check-update") | Some("update") => {
            update_check_command(args.collect())
        }
        Some("case-export") => case_export_command(args.collect()),
        Some("case-import") => case_import_command(args.collect()),
        Some("case-verify") => case_verify_command(args.collect()),
        Some("preflight") => preflight_command(args.collect()),
        Some("mounts") | Some("mount-list") => mount_list_command(),
        Some("mount-cleanup") => mount_cleanup_command(args.collect()),
        Some("ui") => server::run_native(),
        Some("ui-browser") => server::run_browser(),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        None => default_command(),
        Some(other) => Err(format!(
            "{} {}",
            t_cli("Bilinmeyen komut:", "Unknown command:"),
            other
        )),
    };

    if let Err(err) = result {
        report_fatal_error(&err);
        eprintln!("{err}");
        if should_print_help_on_error(&err) {
            print_help();
        }
        std::process::exit(2);
    }
}

/// Sadece kullanıcı komutu yanlış yazdığında genel yardım basar.
fn should_print_help_on_error(err: &str) -> bool {
    err.starts_with("Kullanim:")
        || err.starts_with("Bilinmeyen komut:")
        || err.starts_with("Usage:")
        || err.starts_with("Unknown command:")
}

#[cfg(target_os = "windows")]
fn default_command() -> Result<(), String> {
    server::run_native()
}

/// Windows dışındaki sistemlerde argüman verilmezse sadece yardım metnini gösterir.
#[cfg(not(target_os = "windows"))]
fn default_command() -> Result<(), String> {
    print_help();
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_error_reporting() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        windows_error::report(&format!(
            "Unexpected Amele startup crash:\n\n{info}\n\nLocation: {location}"
        ));
    }));
}

#[cfg(not(target_os = "windows"))]
fn install_error_reporting() {}

#[cfg(target_os = "windows")]
fn report_fatal_error(message: &str) {
    windows_error::report(message);
}

#[cfg(not(target_os = "windows"))]
fn report_fatal_error(_message: &str) {}

/// Windows başlangıç hatalarını log dosyasına ve mesaj kutusuna yazdırır.
#[cfg(target_os = "windows")]
mod windows_error {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    pub fn report(message: &str) {
        let log_path = log_path();
        if let Some(path) = &log_path {
            write_log(path, message);
        }

        let mut body = format!("Amele Forensic Tool could not start.\n\n{message}");
        if let Some(path) = log_path {
            body.push_str(&format!("\n\nLog file:\n{}", path.display()));
        }
        show_message(&body);
    }

    fn write_log(path: &PathBuf, message: &str) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_secs())
                .unwrap_or_default();
            let _ = writeln!(file, "[{ts}] {message}");
        }
    }

    fn log_path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
            .map(|base| base.join("Amele").join("amele.log"))
    }

    fn show_message(message: &str) {
        let title = wide_null("Amele Forensic Tool");
        let body = wide_null(message);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// Kullanıcıya desteklenen teknik CLI komutlarını gösterir.
fn print_help() {
    if is_cli_english() {
        println!(
            "Amele Forensic Tool CLI\n\n\
             User commands:\n\
              ui                                      Open native application window\n\
              ui-browser                              Open in browser for debugging\n\
               profiles                               List local profiles\n\
               profile-create <name> <user> [tr|en] [dark|light] [--direct]\n\
               profile-use <user> [--direct]           Select profile for CLI/next launch\n\
               profile-logout                          Disable automatic profile login\n\
               profile-online-sync                     Synchronize active online profile\n\
               --profile <user> <command>              Execute single command under this profile\n\
               disk-list                               List local disks\n\
               local-image <src> <case> [disk_name] [raw|aff4]  Acquire local disk/file image\n\
               local-ram <avml|winpmem> <case> [tool] [raw|aff4] Acquire local RAM image\n\
               remote-disks <ip> <port> [token]        List remote agent disks\n\
               remote-image <ip> <port> <disk_id> <out_dir> [token] [raw|aff4]\n\
               remote-ram <ip> <port> <case> [token] [raw|aff4] Acquire remote agent RAM image\n\
               adb-status                              Check ADB installation status\n\
               adb-install                             Install ADB via system package manager\n\
               android-devices                         List connected Android devices\n\
               android-profile <serial>                Print Android device profile information\n\
               android-logical <serial> <case> [quick|full|root|volatile]\n\
               android-filesystem <serial> <case> [--root]\n\
               android-ram <serial> <case> [volatile|root|physical] [--root]\n\
               android-capabilities <serial>            Android acquisition compatibility report\n\
               android-lemon-preflight <serial>         Lemon physical RAM compatibility check\n\
               android-remote-connect <host> [port] [tcp|mesh] [label]\n\
               android-remote-disconnect <serial>       Disconnect remote Android ADB connection\n\
               android-case-analysis <case>             Android case output analysis summary\n\
               ios-backup-profile <backup_dir>          Read iOS backup metadata\n\
               ios-backup-normalize <backup_dir> <case> Normalize iOS backup into case ios folder\n\
               docker-status [custom_root]             Inspect Docker daemon or mounted root\n\
               docker-list [custom_root]               List containers, escape risks and secrets\n\
               docker-logs <id> [tail] [custom_root]   Read container logs\n\
               docker-acquire <id> [case] [custom_root] Acquire UpperDir diff + configs + logs\n\
               docker-remote-status <ip> <port> [token] Check remote Docker status via agent\n\
               docker-remote-list <ip> <port> [token]  List remote containers via agent\n\
               docker-remote-logs <ip> <port> <id> [tail] [token] Fetch remote container logs\n\
               docker-remote-acquire <ip> <port> <id> [case] [token] Acquire remote container evidence\n\
               image-analyze <image> [mount_dir]       Disk image analysis summary\n\
               ram-summary <ram> <windows|linux> [symbols]\n\
               ram-strings <ram>                       RAM IOC/string scan\n\
               ram-carve <ram> <out_dir>               RAM file carving\n\
               ram-processes <ram> <windows|linux> [symbols]\n\
               hash <file> [algorithm]                 Calculate md5/sha1/sha256/sha512 hash\n\
               verify <image> <sha256>                 Verify SHA256 image checksum\n\
               wireguard-config <file>                 Generate default WireGuard config\n\
               update-check [--json]                   Check for software updates (AppImage, deb, rpm, arch, msi)\n\n\
             Commands:\n\
               settings-default              Print default settings as JSON\n\
               disk-list-helper <json>        Authorized disk listing helper command\n\
               image-helper <req> <res> <prg> [ctrl] Authorized image acquisition helper command\n\
               ram-helper <req> <res> <prg> <ctrl> Authorized RAM acquisition helper command\n\
               avml-install-helper <src> <res> Authorized AVML installation helper command\n\
               winpmem-install-helper <src> <res> Authorized WinPMEM installation helper command\n\
               mount-helper <req> <res>       Authorized image mount helper command\n\
               disk-size <device|file>       Get disk or file size\n\
               case-export <case> [file]     Export case to .amelecase\n\
               case-import <file>            Import .amelecase package\n\
               case-verify <file>            Verify .amelecase integrity\n\
               remote-tool-check <ip> <port> <winpmem|avml> [token]\n\
               ram-status                    Print local AVML/WinPMEM status\n\
               preflight <src> <type> [tgt]  Disk preflight check\n\
               mounts                        List active mounts\n\
               mount-cleanup [case]          Cleanup mounts\n\n\
             Note: Android/iOS CLI commands require a connected online profile.\n\
             Note: the main binary command is amele-forensic-tool; amele alias is also available for backward compatibility."
        );
    } else {
        println!(
            "Amele Forensic Tool CLI\n\n\
             Kullanici komutlari:\n\
              ui                                      Native uygulama penceresini ac\n\
              ui-browser                              Debug icin tarayicida ac\n\
               profiles                                Yerel profilleri listele\n\
               profile-create <isim> <kullanici> [tr|en] [dark|light] [--direct]\n\
               profile-use <kullanici> [--direct]      CLI/sonraki acilis icin profil sec\n\
               profile-logout                          Otomatik profil acilisini kapat\n\
               profile-online-sync                     Aktif online profili senkronize et\n\
               --profile <kullanici> <komut>           Tek komutu bu profil altinda calistir\n\
               disk-list                               Yerel diskleri listele\n\
               local-image <kaynak> <vaka> [disk_adı] [raw|aff4]  Yerel disk/dosya imaji al\n\
               local-ram <avml|winpmem> <vaka> [arac] [raw|aff4] Yerel RAM imaji al\n\
               remote-disks <ip> <port> [token]        Uzak agent disklerini listele\n\
               remote-image <ip> <port> <disk_id> <cikti_klasoru> [token] [raw|aff4]\n\
               remote-ram <ip> <port> <vaka> [token] [raw|aff4] Uzak agent RAM imaji al\n\
               adb-status                              ADB kurulumunu kontrol et\n\
               adb-install                             ADB'yi sistem paket yöneticisiyle kur\n\
               android-devices                         Android cihazlarini listele\n\
               android-profile <serial>                Android cihaz profilini yazdir\n\
               android-logical <serial> <vaka> [quick|full|root|volatile]\n\
               android-filesystem <serial> <vaka> [--root]\n\
               android-ram <serial> <vaka> [volatile|root|physical] [--root]\n\
               android-capabilities <serial>            Android edinim uygunluk raporu\n\
               android-lemon-preflight <serial>         Lemon fiziksel RAM uygunluk kontrolu\n\
               android-remote-connect <host> [port] [tcp|mesh] [etiket]\n\
               android-remote-disconnect <serial>       Uzak Android ADB baglantisini kes\n\
               android-case-analysis <vaka>             Android vaka cikti analiz ozeti\n\
               ios-backup-profile <backup_klasoru>      iOS backup metadata bilgisini yazdir\n\
               ios-backup-normalize <backup_klasoru> <vaka> iOS backup'i vaka ios klasorune normalize et\n\
               docker-status [kok_dizin]               Docker daemon veya bagli kok dizini denetle\n\
               docker-list [kok_dizin]                 Konteynerleri, riskleri ve secretlari listele\n\
               docker-logs <id> [tail] [kok_dizin]     Konteyner loglarini oku\n\
               docker-acquire <id> [vaka] [kok_dizin]  UpperDir drift + config + loglari vakaya edin\n\
               docker-remote-status <ip> <port> [token] Agent ile uzak Docker durumunu sorgula\n\
               docker-remote-list <ip> <port> [token]  Agent ile uzak konteynerleri listele\n\
               docker-remote-logs <ip> <port> <id> [tail] [token] Uzak konteyner loglarini cek\n\
               docker-remote-acquire <ip> <port> <id> [vaka] [token] Uzak konteyner delillerini vakaya aktar\n\
               image-analyze <imaj> [mount_klasoru]    Disk imaj analiz ozeti\n\
               ram-summary <ram> <windows|linux> [symbols]\n\
               ram-strings <ram>                       RAM IOC/dizgi taramasi\n\
               ram-carve <ram> <cikti_klasoru>         RAM dosya carving\n\
               ram-processes <ram> <windows|linux> [symbols]\n\
               hash <dosya> [algoritma]                md5/sha1/sha256/sha512 hash hesapla\n\
               verify <imaj> <sha256>                  SHA256 imaj dogrulama yap\n\
               wireguard-config <dosya>                Varsayilan WireGuard config uret\n\
               update-check [--json]                   Guncelleme olup olmadigini kontrol et (AppImage, deb, rpm, arch, msi)\n\n\
             Komutlar:\n\
               settings-default              Varsayilan ayarlari JSON olarak yazdir\n\
               disk-list-helper <json>        Yetkili disk listeleme yardimci komutu\n\
               image-helper <req> <res> <prg> [ctrl] Yetkili imaj alma yardimci komutu\n\
               ram-helper <req> <res> <prg> <ctrl> Yetkili RAM alma yardimci komutu\n\
               avml-install-helper <kaynak> <res> Yetkili AVML kurulum yardimci komutu\n\
               winpmem-install-helper <kaynak> <res> Yetkili WinPMEM kurulum yardimci komutu\n\
               mount-helper <req> <res>       Yetkili imaj mount yardimci komutu\n\
               disk-size <cihaz|dosya>       Disk veya dosya boyutu al\n\
               case-export <vaka> [dosya]    Vakayı .amelecase dosyasına aktar\n\
               case-import <dosya>           .amelecase paketini içeri aktar\n\
               case-verify <dosya>           .amelecase bütünlüğünü doğrula\n\
               remote-tool-check <ip> <port> <winpmem|avml> [token]\n\
               ram-status                    Yerel AVML/WinPMEM durumunu yazdir\n\
               preflight <src> <tip> [hedef] Disk preflight kontrolü\n\
               mounts                        Aktif mount'ları listele\n\
               mount-cleanup [vaka]          Mount temizliği yap\n\n\
             Not: Android/iOS CLI komutlari bagli online profil gerektirir.\n\
             Not: paketlerde ana komut amele-forensic-tool'dur; geriye uyumluluk icin amele alias'i da bulunabilir."
        );
    }
}

/// Varsayılan uygulama ayarlarını JSON olarak stdout'a yazar.
fn print_default_settings() -> Result<(), String> {
    let settings = AppSettings::default();
    println!(
        "{}",
        serde_json::to_string_pretty(&settings).map_err(|err| err.to_string())?
    );
    Ok(())
}

/// Yerel profilleri JSON olarak listeler.
fn profile_list_command() -> Result<(), String> {
    let state = amele::profile::bootstrap_profiles().map_err(|err| err.to_string())?;
    print_json(&json!({
        "profiles": state.profiles,
        "active_profile": state.active_profile,
        "base_dir": state.base_dir,
    }))
}

/// CLI üzerinden yeni profil oluşturur.
fn profile_create_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let open_directly = remove_direct_flag(&mut args);
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: profile-create <isim_soyisim> <kullanici> [tr|en] [dark|light] [--direct]",
            "Usage: profile-create <full_name> <username> [tr|en] [dark|light] [--direct]",
        ));
    }
    let language = args.get(2).map(String::as_str).unwrap_or("tr");
    let theme = args.get(3).map(String::as_str).unwrap_or("dark");
    let profile =
        amele::profile::create_profile(&args[0], &args[1], language, theme, open_directly)
            .map_err(|err| err.to_string())?;
    print_json(&profile)
}

/// CLI üzerinden mevcut profili seçer.
fn profile_use_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let open_directly = remove_direct_flag(&mut args);
    if args.is_empty() {
        return Err(t_cli(
            "Kullanim: profile-use <kullanici> [--direct]",
            "Usage: profile-use <username> [--direct]",
        ));
    }
    let profile =
        amele::profile::select_profile(&args[0], open_directly).map_err(|err| err.to_string())?;
    print_json(&profile)
}

/// Aktif/otomatik profil seçimini kapatır.
fn profile_logout_command() -> Result<(), String> {
    amele::profile::logout_profile().map_err(|err| err.to_string())?;
    print_json(&json!({ "ok": true }))
}

/// CLI üzerinden aktif profilin online bilgilerini yeniler.
fn profile_online_sync_command() -> Result<(), String> {
    let profile = amele::profile::sync_active_online_profile()
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&json!({
        "profile": profile,
        "access": amele::profile::mobile_tools_access(),
    }))
}

/// Verilen dosya için seçilen hash algoritmasını çalıştırır.
fn hash_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(t_cli(
            "Kullanim: hash <dosya> [algoritma]",
            "Usage: hash <file> [algorithm]",
        ));
    }
    let path = PathBuf::from(&args[0]);
    let algorithm = args
        .get(1)
        .and_then(|value| HashAlgorithm::parse(value))
        .unwrap_or(HashAlgorithm::Sha256);
    let value = hash::calculate_file_hash(&path, algorithm).map_err(|err| err.to_string())?;
    println!("{}  {}", algorithm.name(), value);
    Ok(())
}

/// Yerel disk listesini JSON olarak üretir.
fn disk_list_command() -> Result<(), String> {
    let disks = disk::list_disks().map_err(|err| err.to_string())?;
    print_json(&disks)
}

/// Yerel disk veya dosya kaynağını vaka klasörüne imaj olarak yazar.
fn local_image_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let selected_format = extract_output_format(&mut args)?;
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: local-image <kaynak> <vaka> [disk_adı] [raw|aff4]",
            "Usage: local-image <source> <case> [disk_name] [raw|aff4]",
        ));
    }
    let source = PathBuf::from(&args[0]);
    let vault = cli_case_vault(&args[1])?;
    let disk_name = args.get(2).map(String::as_str).unwrap_or_else(|| {
        source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("disk")
    });
    let raw_target = vault.outputs_dir.join(format!(
        "{}_{}.img",
        cli_safe_stem(disk_name),
        cli_timestamp()
    ));
    let plan = output_format::plan_output(&raw_target, selected_format);
    let task = disk::DiskAcquisitionTask::new(&source, &plan.working_path);
    let result = disk::run_disk_acquisition(&task, |done, total| {
        print_progress("imaj", done, total);
    })
    .map_err(|err| crate_diagnostic(err.to_string()))?;
    let finalized = output_format::finalize_output(
        &plan,
        "disk",
        disk_name,
        &vault.case_name,
        result.sha256.clone(),
    )?;
    print_json(&json!({
        "case": vault.case_name,
        "target_path": finalized.target_path,
        "bytes_copied": result.bytes_copied,
        "total_bytes": result.total_bytes,
        "sha256": finalized.sha256,
        "raw_sha256": finalized.raw_sha256,
        "output_format": finalized.format.as_str(),
    }))
}

/// AVML veya WinPMEM ile yerel RAM imajı alır.
fn local_ram_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let selected_format = extract_output_format(&mut args)?;
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: local-ram <avml|winpmem> <vaka> [arac_yolu] [raw|aff4]",
            "Usage: local-ram <avml|winpmem> <case> [tool_path] [raw|aff4]",
        ));
    }
    let tool = args[0].to_ascii_lowercase();
    let vault = cli_case_vault(&args[1])?;
    let raw_target = vault.ram_dir.join(format!("ram_{}.raw", cli_timestamp()));
    let plan = output_format::plan_output(&raw_target, selected_format);
    let candidate = args.get(2).map(Path::new).filter(|path| path.exists());
    let token = ram::CancellationToken::default();
    let result = match tool.as_str() {
        "avml" => ram::acquire_with_avml(&plan.working_path, candidate, &token, |done, total| {
            print_progress("ram", done, total);
        }),
        "winpmem" => {
            ram::acquire_with_winpmem(&plan.working_path, candidate, &token, |done, total| {
                print_progress("ram", done, total);
            })
        }
        _ => return Err("tool must be avml or winpmem".to_string()),
    }
    .map_err(|err| crate_diagnostic(err.to_string()))?;
    let finalized = output_format::finalize_output(&plan, "ram", &tool, &vault.case_name, None)?;
    print_json(&json!({
        "case": vault.case_name,
        "target_path": finalized.target_path,
        "bytes_written": result.bytes_written,
        "sha256": finalized.sha256,
        "raw_sha256": finalized.raw_sha256,
        "output_format": finalized.format.as_str(),
    }))
}

/// Uzak agent üzerinde RAM edinimini başlatır ve sonucu vaka klasörüne indirir.
fn remote_ram_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let selected_format = extract_output_format(&mut args)?;
    if args.len() < 3 {
        return Err(t_cli(
            "Kullanim: remote-ram <ip> <port> <vaka> [token] [raw|aff4]",
            "Usage: remote-ram <ip> <port> <case> [token] [raw|aff4]",
        ));
    }
    let ip = &args[0];
    let port = parse_port(&args[1])?;
    let vault = cli_case_vault(&args[2])?;
    let token = args.get(3).cloned();
    let target = vault
        .ram_dir
        .join(format!("{}_ram_{}.raw", cli_safe_stem(ip), cli_timestamp()));
    let plan = output_format::plan_output(&target, selected_format);
    let remote_file = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("remote_ram.raw")
        .to_string();

    let mut connection = RemoteConnection::connect(ip, port, token)
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    let job_id = format!("cli-{}", std::process::id());
    let remote_result = connection
        .start_remote_ram(
            &remote_file,
            Some(&job_id),
            selected_format,
            |done, total| {
                print_progress("remote-ram", done, total);
            },
        )
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    let download = connection
        .download_ram_file(
            &remote_file,
            &plan.working_path,
            Some(&job_id),
            |done, total| {
                print_progress("download", done, total);
            },
        )
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    let finalized = output_format::finalize_output(
        &plan,
        "ram",
        ip,
        &vault.case_name,
        download.sha256.or(remote_result.sha256),
    )?;
    print_json(&json!({
        "case": vault.case_name,
        "remote_job_id": remote_result.job_id,
        "target_path": finalized.target_path,
        "bytes_transferred": download.bytes_transferred,
        "remote_bytes": remote_result.total_size,
        "sha256": finalized.sha256,
        "raw_sha256": finalized.raw_sha256,
        "output_format": finalized.format.as_str(),
    }))
}

/// Disk imajını mount olmadan yapısal olarak analiz eder.
fn image_analyze_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanim: image-analyze <imaj> [mount_klasoru]".to_string());
    }
    let image_path = PathBuf::from(&args[0]);
    let mount_dir = args.get(1).map(PathBuf::from);
    let report = disk_analysis::analyze_disk_image(&image_path, mount_dir.as_deref())
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&report)
}

/// RAM imajı için özet analiz üretir.
fn ram_summary_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err("Kullanim: ram-summary <ram> <windows|linux> [symbols]".to_string());
    }
    let path = PathBuf::from(&args[0]);
    let symbols = args.get(2).map(PathBuf::from);
    let summary = ram_analysis::analyze_ram_summary_logged_with_symbol_dir(
        &path,
        Some(args[1].as_str()),
        symbols.as_deref(),
        None,
    )
    .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&summary)
}

/// RAM imajında IOC/dizgi taraması yapar.
fn ram_strings_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanim: ram-strings <ram>".to_string());
    }
    let matches = ram_analysis::analyze_ram_strings(Path::new(&args[0]))
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&matches)
}

/// RAM içinden sınırlı dosya carving yapar.
fn ram_carve_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err("Kullanim: ram-carve <ram> <cikti_klasoru>".to_string());
    }
    let files = ram_analysis::carve_files(Path::new(&args[0]), Path::new(&args[1]))
        .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&files)
}

/// Volatility3 ile proses listesini çıkarmaya çalışır.
fn ram_processes_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err("Kullanim: ram-processes <ram> <windows|linux> [symbols]".to_string());
    }
    let symbols = args.get(2).map(PathBuf::from);
    let processes = amele::volatility::get_processes_logged_with_symbol_dir(
        Path::new(&args[0]),
        &args[1],
        symbols.as_deref(),
        None,
    )
    .map_err(crate_diagnostic)?;
    print_json(&processes)
}

/// ADB kurulum durumunu JSON olarak yazar.
fn android_adb_status_command() -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    print_json(&android::adb_status())
}

/// ADB'yi uygun paket yöneticisiyle kurmayı dener.
fn android_adb_install_command() -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    let result = android::install_adb()
        .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&result)
}

/// ADB ile bağlı Android cihazlarını listeler.
fn android_devices_command() -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    let devices = android::list_devices()
        .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&devices)
}

fn require_cli_mobile_tools_access() -> Result<(), String> {
    amele::profile::require_mobile_tools_access().map_err(|err| crate_diagnostic(err.to_string()))
}

/// Android cihaz profilini çıkarır.
fn android_profile_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: android-profile <serial>".to_string());
    }
    let profile = android::detect_device_profile(&args[0])
        .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&profile)
}

/// Android mantıksal edinimi vaka klasörüne yazar.
fn android_logical_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.len() < 2 {
        return Err("Kullanim: android-logical <serial> <vaka> [quick|full|root]".to_string());
    }
    let profile = args
        .get(2)
        .map(|value| android::AndroidAcquisitionProfile::from_id(value))
        .unwrap_or(android::AndroidAcquisitionProfile::FullLogical);
    let vault = cli_case_vault(&args[1])?;
    let output_dir = vault
        .android_dir
        .join(format!("logical_{}", cli_timestamp()));
    let result = android::orchestrated_acquisition(
        &args[0],
        &output_dir,
        profile,
        |done, total, category| print_step_progress("android-logical", done, total, category),
        || false,
    )
    .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&result)
}

/// Android dosya sistemi edinimini vaka klasörüne yazar.
fn android_filesystem_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.len() < 2 {
        return Err("Kullanim: android-filesystem <serial> <vaka> [--root]".to_string());
    }
    let has_root = args.iter().any(|arg| arg == "--root" || arg == "root");
    let vault = cli_case_vault(&args[1])?;
    let output_dir = vault
        .android_dir
        .join(format!("filesystem_{}", cli_timestamp()));
    let result = android::orchestrated_filesystem_acquisition(
        &args[0],
        &output_dir,
        has_root,
        |done, total, category| print_step_progress("android-filesystem", done, total, category),
        || false,
    )
    .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&result)
}

/// Android uçucu veri/RAM edinimini vaka klasörüne yazar.
fn android_ram_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.len() < 2 {
        return Err(
            "Kullanim: android-ram <serial> <vaka> [volatile|root|physical] [--root]".to_string(),
        );
    }
    let mode = args
        .get(2)
        .filter(|value| !value.starts_with("--"))
        .map(|value| android::AndroidRamMode::from_id(value))
        .unwrap_or(android::AndroidRamMode::VolatileData);
    let has_root = args.iter().any(|arg| arg == "--root" || arg == "root");
    let vault = cli_case_vault(&args[1])?;
    let output_dir = vault.android_dir.join(format!("ram_{}", cli_timestamp()));
    let result = android::orchestrated_ram_acquisition(
        &args[0],
        &output_dir,
        has_root,
        mode,
        |done, total, category| print_step_progress("android-ram", done, total, category),
        || false,
    )
    .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    print_json(&result)
}

/// Android cihazın hangi edinim modlarına uygun olduğunu raporlar.
fn android_capabilities_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: android-capabilities <serial>".to_string());
    }
    let profile = android::detect_device_profile(&args[0])
        .map_err(|err| crate_diagnostic(android::explain_android_error(err)))?;
    let report = android::build_android_capability_report(&args[0], &profile);
    print_json(&json!({
        "profile": profile,
        "capabilities": report,
    }))
}

/// Lemon fiziksel RAM aracı için cihaz ön kontrolünü çalıştırır.
fn android_lemon_preflight_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: android-lemon-preflight <serial>".to_string());
    }
    let report = android::lemon_preflight(&args[0]);
    print_json(&report)
}

/// TCP/IP ADB veya MESH relay endpoint'ine bağlanır.
fn android_remote_connect_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err(
            "Kullanim: android-remote-connect <host> [port] [tcp|mesh] [etiket]".to_string(),
        );
    }
    let host = args[0].clone();
    let port = args
        .get(1)
        .map(|value| parse_port(value))
        .transpose()?
        .unwrap_or(5555);
    let kind = match args.get(2).map(String::as_str) {
        Some("mesh") | Some("mesh_relay") => android::RemoteEndpointKind::MeshRelay,
        _ => android::RemoteEndpointKind::TcpAdb,
    };
    let label = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", host, port));
    let endpoint = android::RemoteAndroidEndpoint {
        label,
        host,
        port,
        kind,
    };
    let result = android::connect_remote_endpoint(&endpoint);
    print_json(&json!({
        "endpoint": endpoint,
        "result": result,
    }))
}

/// Uzak Android ADB bağlantısını keser.
fn android_remote_disconnect_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: android-remote-disconnect <serial>".to_string());
    }
    let result = android::disconnect_remote_endpoint(&args[0]);
    print_json(&result)
}

/// Seçilen vakanın Android çıktılarından analiz özeti üretir.
fn android_case_analysis_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: android-case-analysis <vaka>".to_string());
    }
    let vault = cli_case_vault(&args[0])?;
    let report = android_analysis::analyze_android_case(&vault.case_name, &vault.android_dir);
    print_json(&report)
}

/// iOS backup profil bilgisini JSON olarak yazar.
fn ios_backup_profile_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.is_empty() {
        return Err("Kullanim: ios-backup-profile <backup_klasoru>".to_string());
    }
    let info = ios::inspect_backup(&args[0]).map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&info)
}

/// iOS backup klasörünü vaka ios klasörüne Backup2FS düzeninde normalize eder.
fn ios_backup_normalize_command(args: Vec<String>) -> Result<(), String> {
    require_cli_mobile_tools_access()?;
    if args.len() < 2 {
        return Err("Kullanim: ios-backup-normalize <backup_klasoru> <vaka>".to_string());
    }
    let backup_path = PathBuf::from(&args[0]);
    let vault = cli_case_vault(&args[1])?;
    let backup_name = backup_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(cli_safe_stem)
        .unwrap_or_else(|| "ios_backup".to_string());
    let output_dir = vault
        .ios_dir
        .join(format!("{}_{}", backup_name, cli_timestamp()));
    let result = ios::normalize_backup(
        &backup_path,
        &output_dir,
        &[
            HashAlgorithm::Md5,
            HashAlgorithm::Sha1,
            HashAlgorithm::Sha256,
        ],
        |done, total, step| print_step_progress("ios-backup", done as u32, total as u32, step),
        |line| eprintln!("ios-backup: {line}"),
        || false,
        || false,
    )
    .map_err(|err| crate_diagnostic(err.to_string()))?;
    print_json(&result)
}

/// Docker sistem durumunu veya bağlanmış dizini denetler.
fn docker_status_command(args: Vec<String>) -> Result<(), String> {
    let custom_root = args.first().map(Path::new);
    let status = docker::check_docker_status(custom_root);
    print_json(&status)
}

/// Docker konteynerlerini listeler ve güvenlik/kaçış risk analizini gösterir.
fn docker_list_command(args: Vec<String>) -> Result<(), String> {
    let custom_root = args.first().map(Path::new);
    let containers = docker::list_containers(custom_root).map_err(|err| err.to_string())?;
    print_json(&containers)
}

/// Belirtilen konteynerin log kayıtlarını ekrana basar.
fn docker_logs_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(t_cli(
            "Kullanim: docker-logs <konteyner_id> [tail] [kok_dizin]",
            "Usage: docker-logs <container_id> [tail] [custom_root]",
        ));
    }
    let cid = &args[0];
    let tail = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let custom_root = args.get(2).map(Path::new);
    let logs = docker::get_container_logs(cid, tail, custom_root).map_err(|err| err.to_string())?;
    print_json(&logs)
}

/// Yerel konteyner delillerini (Overlay2 diff, konfigürasyon, loglar) vakaya toplar.
fn docker_acquire_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(t_cli(
            "Kullanim: docker-acquire <konteyner_id> [vaka_adi] [kok_dizin]",
            "Usage: docker-acquire <container_id> [case_name] [custom_root]",
        ));
    }
    let cid = args[0].clone();
    let case_name = args.get(1).cloned();
    let custom_root = args.get(2).cloned();

    let req = DockerAcquisitionRequest {
        container_id: cid,
        acquire_diff: true,
        acquire_logs: true,
        acquire_config: true,
        case_name,
        custom_docker_root: custom_root,
    };

    let base_dir = amele::api::default_case_base_dir();
    let result = docker::acquire_container_evidence(&req, base_dir, |msg, done, total| {
        print_progress("docker", done, total);
        eprintln!("{msg}");
    })
    .map_err(|err| err.to_string())?;

    print_json(&result)
}

/// Uzak agent'tan Docker daemon durumunu çeker.
fn docker_remote_status_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: docker-remote-status <ip> <port> [token]",
            "Usage: docker-remote-status <ip> <port> [token]",
        ));
    }
    let ip = &args[0];
    let port = parse_port(&args[1])?;
    let token = args.get(2).cloned();
    let mut conn = RemoteConnection::connect(ip, port, token).map_err(|err| err.to_string())?;
    let status = conn.docker_status().map_err(|err| err.to_string())?;
    print_json(&status)
}

/// Uzak agent üzerindeki Docker konteynerlerini listeler.
fn docker_remote_list_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: docker-remote-list <ip> <port> [token]",
            "Usage: docker-remote-list <ip> <port> [token]",
        ));
    }
    let ip = &args[0];
    let port = parse_port(&args[1])?;
    let token = args.get(2).cloned();
    let mut conn = RemoteConnection::connect(ip, port, token).map_err(|err| err.to_string())?;
    let containers = conn
        .list_docker_containers()
        .map_err(|err| err.to_string())?;
    print_json(&containers)
}

/// Uzak agent üzerindeki konteynerin loglarını çeker.
fn docker_remote_logs_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 3 {
        return Err(t_cli(
            "Kullanim: docker-remote-logs <ip> <port> <konteyner_id> [tail] [token]",
            "Usage: docker-remote-logs <ip> <port> <container_id> [tail] [token]",
        ));
    }
    let ip = &args[0];
    let port = parse_port(&args[1])?;
    let cid = &args[2];
    let tail = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(200);
    let token = args.get(4).cloned();
    let mut conn = RemoteConnection::connect(ip, port, token).map_err(|err| err.to_string())?;
    let logs = conn
        .get_docker_container_logs(cid, tail)
        .map_err(|err| err.to_string())?;
    print_json(&logs)
}

/// Uzak agent üzerindeki konteyner delillerini vaka klasörüne aktarır.
fn docker_remote_acquire_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 3 {
        return Err(t_cli(
            "Kullanim: docker-remote-acquire <ip> <port> <konteyner_id> [vaka_adi] [token]",
            "Usage: docker-remote-acquire <ip> <port> <container_id> [case_name] [token]",
        ));
    }
    let ip = &args[0];
    let port = parse_port(&args[1])?;
    let cid = &args[2];
    let case_name = args.get(3).cloned().unwrap_or_else(|| {
        format!(
            "VAKA_DOCKER_REMOTE_{}",
            Local::now().format("%Y%m%d_%H%M%S")
        )
    });
    let token = args.get(4).cloned();

    let vault = cli_case_vault(&case_name)?;
    let short_id = if cid.len() >= 12 { &cid[..12] } else { cid };
    let target_dir = vault.docker_dir.join(format!("remote_{}", short_id));
    fs::create_dir_all(&target_dir).map_err(|err| err.to_string())?;
    let target_tar = target_dir.join("docker_evidence.tar.gz");

    let mut conn = RemoteConnection::connect(ip, port, token).map_err(|err| err.to_string())?;
    let result = conn
        .acquire_remote_docker(cid, true, true, true, &target_tar, None, |done, total| {
            print_progress("docker-remote", done, total);
        })
        .map_err(|err| err.to_string())?;

    let meta_path = target_dir.join("docker_metadata.json");
    let _ = fs::write(
        &meta_path,
        json!({
            "edinim_zamani": Local::now().to_rfc3339(),
            "konteyner_id": cid,
            "isim": format!("remote_{}", short_id),
            "sha256": result.sha256,
            "boyut": result.bytes_transferred,
        })
        .to_string(),
    );

    let manifest_path = target_dir.join("manifest.csv");
    let manifest_content = format!(
        "Dosya_Adi,Boyut_Byte,SHA256\ndocker_evidence.tar.gz,{},{}\n",
        result.bytes_transferred,
        result.sha256.as_deref().unwrap_or("HATA")
    );
    let _ = fs::write(&manifest_path, manifest_content);

    print_json(&result)
}

/// Yetkili helper sürecinde diskleri listeleyip sonucu dosyaya yazar.
fn disk_list_helper_command(args: Vec<String>) -> Result<(), String> {
    let Some(output) = args.first() else {
        return Err("Kullanim: disk-list-helper <json-cikti>".to_string());
    };
    let payload = match disk::list_disks() {
        Ok(disks) => json!({ "ok": true, "disks": disks }),
        Err(err) => json!({ "ok": false, "error": err.to_string() }),
    };
    fs::write(
        output,
        serde_json::to_vec_pretty(&payload).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

/// Yetkili imaj alma helper'ının JSON istek alanlarını taşır.
#[derive(Deserialize)]
struct ImageHelperRequest {
    source: PathBuf,
    target: PathBuf,
    owner_uid: Option<u32>,
    owner_gid: Option<u32>,
}

/// Root/admin yetkisiyle disk imajı alır ve ilerlemeyi/result dosyalarını günceller.
fn image_helper_command(args: Vec<String>) -> Result<(), String> {
    if !(3..=4).contains(&args.len()) {
        return Err(
            "Kullanim: image-helper <request-json> <result-json> <progress-json> [control-json]"
                .to_string(),
        );
    }
    let request_path = PathBuf::from(&args[0]);
    let result_path = PathBuf::from(&args[1]);
    let progress_path = PathBuf::from(&args[2]);
    let control_path = args.get(3).map(PathBuf::from);
    let request: ImageHelperRequest =
        serde_json::from_slice(&fs::read(&request_path).map_err(|err| err.to_string())?)
            .map_err(|err| err.to_string())?;

    let task = disk::DiskAcquisitionTask::new(&request.source, &request.target);
    let result = disk::run_disk_acquisition_with_control(
        &task,
        |done, total| {
            let _ = write_json_file(
                &progress_path,
                &json!({
                    "done": done,
                    "total": total,
                    "message": "Imaj alma suruyor",
                }),
            );
        },
        || image_helper_control(control_path.as_deref()),
    );

    let payload = match result {
        Ok(result) => {
            restore_helper_output_owner(&result.target, request.owner_uid, request.owner_gid);
            json!({
                "ok": true,
                "target_path": result.target,
                "bytes_copied": result.bytes_copied,
                "total_bytes": result.total_bytes,
                "sha256": result.sha256,
            })
        }
        Err(err) => json!({
            "ok": false,
            "error": err.to_string(),
        }),
    };
    write_json_file(&result_path, &payload)
}

/// Helper root olarak çalıştıysa çıkan dosyaları asıl kullanıcıya geri verir.
fn restore_helper_output_owner(target: &Path, owner_uid: Option<u32>, owner_gid: Option<u32>) {
    let (Some(owner_uid), Some(owner_gid)) = (owner_uid, owner_gid) else {
        return;
    };
    for path in [target.to_path_buf(), sha256_sidecar_path(target)] {
        if path.exists() {
            let _ = Command::new("chown")
                .arg(format!("{owner_uid}:{owner_gid}"))
                .arg(path)
                .output();
        }
    }
}

/// İmaj dosyasının yanında oluşturulan SHA256 sidecar yolunu hesaplar.
fn sha256_sidecar_path(target: &Path) -> PathBuf {
    target.with_extension(format!(
        "{}sha256",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ))
}

/// UI'dan gelen pause/resume/stop kontrol dosyasını disk edinim durumuna çevirir.
fn image_helper_control(control_path: Option<&Path>) -> disk::DiskAcquisitionControl {
    let Some(control_path) = control_path else {
        return disk::DiskAcquisitionControl::Continue;
    };
    let Some(value) = fs::read(control_path)
        .ok()
        .and_then(|payload| serde_json::from_slice::<Value>(&payload).ok())
    else {
        return disk::DiskAcquisitionControl::Continue;
    };
    match value
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "cancelled" | "cancel" | "stop" => disk::DiskAcquisitionControl::Cancel,
        "paused" | "pause" => disk::DiskAcquisitionControl::Pause,
        _ => disk::DiskAcquisitionControl::Continue,
    }
}

/// Yetkili RAM helper'ının çalıştıracağı araç ve çıktı bilgilerini taşır.
#[derive(Deserialize)]
struct RamHelperRequest {
    output_file: PathBuf,
    tool: String,
    tool_path: Option<PathBuf>,
    owner_uid: Option<u32>,
    owner_gid: Option<u32>,
}

/// Root/admin yetkisiyle AVML veya WinPMEM çalıştırıp RAM çıktısını üretir.
fn ram_helper_command(args: Vec<String>) -> Result<(), String> {
    if args.len() != 4 {
        return Err(
            "Kullanim: ram-helper <request-json> <result-json> <progress-json> <control-json>"
                .to_string(),
        );
    }
    let request_path = PathBuf::from(&args[0]);
    let result_path = PathBuf::from(&args[1]);
    let progress_path = PathBuf::from(&args[2]);
    let control_path = PathBuf::from(&args[3]);
    let request: RamHelperRequest =
        serde_json::from_slice(&fs::read(&request_path).map_err(|err| err.to_string())?)
            .map_err(|err| err.to_string())?;

    let token = ram::CancellationToken::default();
    let watcher_stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let token = token.clone();
        let watcher_stop = watcher_stop.clone();
        thread::spawn(move || {
            while !watcher_stop.load(Ordering::SeqCst) {
                apply_ram_helper_control(&token, &control_path);
                thread::sleep(Duration::from_millis(200));
            }
        })
    };

    let candidate = request.tool_path.as_deref();
    let result = match request.tool.as_str() {
        "avml" => ram::acquire_with_avml(&request.output_file, candidate, &token, |done, total| {
            let _ = write_json_file(
                &progress_path,
                &json!({
                    "done": done,
                    "total": total,
                    "message": "RAM edinimi suruyor",
                }),
            );
        }),
        "winpmem" => {
            ram::acquire_with_winpmem(&request.output_file, candidate, &token, |done, total| {
                let _ = write_json_file(
                    &progress_path,
                    &json!({
                        "done": done,
                        "total": total,
                        "message": "RAM edinimi suruyor",
                    }),
                );
            })
        }
        _ => Err(amele::error::AmeleError::new(
            amele::error::HataKodu::Genel,
            "Desteklenmeyen RAM araci",
        )),
    };

    watcher_stop.store(true, Ordering::SeqCst);
    let _ = watcher.join();

    let payload = match result {
        Ok(result) => {
            restore_helper_output_owner(&result.output_file, request.owner_uid, request.owner_gid);
            json!({
                "ok": true,
                "target_path": result.output_file,
                "bytes_written": result.bytes_written,
            })
        }
        Err(err) => json!({
            "ok": false,
            "error": err.to_string(),
        }),
    };
    write_json_file(&result_path, &payload)
}

/// RAM helper kontrol dosyasındaki pause/resume/stop durumunu token'a uygular.
fn apply_ram_helper_control(token: &ram::CancellationToken, control_path: &Path) {
    let Some(value) = fs::read(control_path)
        .ok()
        .and_then(|payload| serde_json::from_slice::<Value>(&payload).ok())
    else {
        return;
    };
    match value
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "cancelled" | "cancel" | "stop" => token.cancel(),
        "paused" | "pause" => token.pause(),
        "running" | "resume" => token.resume(),
        _ => {}
    }
}

fn avml_install_helper_command(args: Vec<String>) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Kullanim: avml-install-helper <kaynak> <result-json>".to_string());
    }
    let source = PathBuf::from(&args[0]);
    let result_path = PathBuf::from(&args[1]);
    let payload = match install_avml_binary(&source) {
        Ok(value) => value,
        Err(err) => json!({
            "ok": false,
            "error": err,
        }),
    };
    write_json_file(&result_path, &payload)
}

#[cfg(target_os = "linux")]
fn install_avml_binary(source: &Path) -> Result<Value, String> {
    use std::os::unix::fs::PermissionsExt;

    if !source.is_file() {
        return Err("Downloaded AVML binary not found".to_string());
    }

    let target = Path::new("/usr/bin/avml");
    let temp = Path::new("/usr/bin/.amele-avml.tmp");
    fs::copy(source, temp).map_err(|err| format!("AVML /usr/bin altina kopyalanamadi: {err}"))?;
    let mut permissions = fs::metadata(temp)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(temp, permissions).map_err(|err| err.to_string())?;
    let _ = Command::new("chown").arg("root:root").arg(temp).status();
    fs::rename(temp, target)
        .map_err(|err| format!("AVML /usr/bin/avml olarak kurulamadi: {err}"))?;

    let version = Command::new(target)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(json!({
        "ok": true,
        "path": target,
        "version": version,
        "message": "AVML /usr/bin/avml olarak kuruldu",
    }))
}

#[cfg(not(target_os = "linux"))]
fn install_avml_binary(_source: &Path) -> Result<Value, String> {
    Err("AVML installation is only supported on Linux".to_string())
}

fn winpmem_install_helper_command(args: Vec<String>) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Kullanim: winpmem-install-helper <kaynak> <result-json>".to_string());
    }
    let source = PathBuf::from(&args[0]);
    let result_path = PathBuf::from(&args[1]);
    let payload = match install_winpmem_binary(&source) {
        Ok(value) => value,
        Err(err) => json!({
            "ok": false,
            "error": err,
        }),
    };
    write_json_file(&result_path, &payload)
}

#[cfg(windows)]
fn install_winpmem_binary(source: &Path) -> Result<Value, String> {
    if !source.is_file() {
        return Err("Downloaded WinPMEM binary not found".to_string());
    }

    let target_dir = Path::new(r"C:\Tools");
    fs::create_dir_all(target_dir).map_err(|err| format!("C:\\Tools olusturulamadi: {err}"))?;
    let target = target_dir.join(ram::WINPMEM_NAME);
    let temp = target_dir.join(".amele-winpmem.tmp");
    fs::copy(source, &temp)
        .map_err(|err| format!("WinPMEM C:\\Tools altina kopyalanamadi: {err}"))?;
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|err| format!("Eski WinPMEM dosyasi kaldirilamadi: {err}"))?;
    }
    fs::rename(&temp, &target)
        .map_err(|err| format!("WinPMEM C:\\Tools altina kurulamadi: {err}"))?;

    Ok(json!({
        "ok": true,
        "path": target,
        "message": "WinPMEM C:\\Tools altina kuruldu",
    }))
}

#[cfg(not(windows))]
fn install_winpmem_binary(_source: &Path) -> Result<Value, String> {
    Err("WinPMEM installation is only supported on Windows".to_string())
}

#[derive(Deserialize)]
struct MountHelperRequest {
    action: String,
    image_path: Option<PathBuf>,
    mount_dir: PathBuf,
    loop_device: Option<PathBuf>,
}

fn mount_helper_command(args: Vec<String>) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Kullanim: mount-helper <request-json> <result-json>".to_string());
    }
    let request_path = PathBuf::from(&args[0]);
    let result_path = PathBuf::from(&args[1]);
    let request: MountHelperRequest =
        serde_json::from_slice(&fs::read(&request_path).map_err(|err| err.to_string())?)
            .map_err(|err| err.to_string())?;

    let result = match request.action.as_str() {
        "mount" => {
            let image_path = request
                .image_path
                .as_deref()
                .ok_or_else(|| "image_path is required".to_string());
            image_path.and_then(|image_path| mount_image_readonly(image_path, &request.mount_dir))
        }
        "unmount" => unmount_image(&request.mount_dir, request.loop_device.as_deref())
            .map(|_| json!({ "ok": true, "mount_dir": request.mount_dir })),
        _ => Err("action must be mount or unmount".to_string()),
    };

    let payload = match result {
        Ok(value) => value,
        Err(err) => json!({ "ok": false, "error": err }),
    };
    write_json_file(&result_path, &payload)
}

#[cfg(target_os = "linux")]
fn mount_image_readonly(image_path: &Path, mount_dir: &Path) -> Result<Value, String> {
    let direct = Command::new("mount")
        .arg("-o")
        .arg("ro,loop")
        .arg(image_path)
        .arg(mount_dir)
        .output()
        .map_err(|err| err.to_string())?;
    if direct.status.success() {
        return Ok(json!({
            "ok": true,
            "mount_dir": mount_dir,
            "loop_device": Value::Null,
        }));
    }

    let direct_error = command_error_message(
        &direct,
        "mount failed; image may contain a partition table or root privileges may be required",
    );
    mount_partitioned_image(image_path, mount_dir)
        .map_err(|err| format!("{direct_error}\npartition scan failed: {err}"))
}

#[cfg(windows)]
fn mount_image_readonly(image_path: &Path, _mount_dir: &Path) -> Result<Value, String> {
    let mount_dir = windows_mount_image_readonly(image_path)?;
    Ok(json!({
        "ok": true,
        "mount_dir": mount_dir,
        "loop_device": Value::Null,
    }))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn mount_image_readonly(_image_path: &Path, _mount_dir: &Path) -> Result<Value, String> {
    Err("image mount helper is not supported on this platform".to_string())
}

#[cfg(windows)]
fn windows_mount_image_readonly(image_path: &Path) -> Result<PathBuf, String> {
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(
            "$ErrorActionPreference='Stop'; \
             $image = $args[0]; \
             Mount-DiskImage -ImagePath $image -Access ReadOnly | Out-Null; \
             Start-Sleep -Milliseconds 500; \
             $diskImage = Get-DiskImage -ImagePath $image; \
             $disk = $diskImage | Get-Disk -ErrorAction Stop; \
             $partition = $disk | Get-Partition | Where-Object { $_.Type -ne 'Reserved' } | Select-Object -First 1; \
             $volume = $partition | Get-Volume -ErrorAction SilentlyContinue; \
             if ($volume -and $volume.DriveLetter) { \
               Write-Output ($volume.DriveLetter + ':\\'); \
               exit 0; \
             }; \
             $accessPath = $partition.AccessPaths | Where-Object { $_ -like '*:\\*' -or $_ -like '\\\\?\\Volume*' } | Select-Object -First 1; \
             if ($accessPath) { \
               Write-Output $accessPath; \
               exit 0; \
             }; \
             Dismount-DiskImage -ImagePath $image -ErrorAction SilentlyContinue; \
             throw 'Mounted image has no drive letter. Windows supports ISO/VHD/VHDX here; raw DD/IMG needs a forensic image driver.'",
        )
        .arg(image_path)
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if stderr.is_empty() {
            if stdout.is_empty() {
                "Windows image mount failed".to_string()
            } else {
                stdout
            }
        } else {
            stderr
        });
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .map(PathBuf::from)
        .ok_or_else(|| {
            "Windows mount succeeded but did not return a readable mount path.".to_string()
        })
}

#[cfg(target_os = "linux")]
fn mount_partitioned_image(image_path: &Path, mount_dir: &Path) -> Result<Value, String> {
    let setup = Command::new("losetup")
        .arg("--find")
        .arg("--partscan")
        .arg("--read-only")
        .arg("--show")
        .arg(image_path)
        .output()
        .map_err(|err| err.to_string())?;
    if !setup.status.success() {
        return Err(command_error_message(
            &setup,
            "losetup failed; root privileges may be required",
        ));
    }

    let loop_device = PathBuf::from(String::from_utf8_lossy(&setup.stdout).trim());
    if loop_device.as_os_str().is_empty() {
        return Err("losetup did not return a loop device".to_string());
    }
    thread::sleep(Duration::from_millis(250));

    let mut last_error = String::new();
    for candidate in loop_mount_candidates(&loop_device) {
        let output = Command::new("mount")
            .arg("-o")
            .arg("ro")
            .arg(&candidate)
            .arg(mount_dir)
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            return Ok(json!({
                "ok": true,
                "mount_dir": mount_dir,
                "loop_device": loop_device,
            }));
        }
        last_error = format!(
            "{}: {}",
            candidate.display(),
            command_error_message(&output, "mount failed")
        );
    }

    let _ = Command::new("losetup").arg("-d").arg(&loop_device).output();
    Err(if last_error.is_empty() {
        "no mountable filesystem partition was found in the image".to_string()
    } else {
        last_error
    })
}

#[cfg(target_os = "linux")]
fn loop_mount_candidates(loop_device: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(output) = Command::new("lsblk")
        .arg("-rnpo")
        .arg("PATH,TYPE")
        .arg(loop_device)
        .output()
        && output.status.success()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut parts = line.split_whitespace();
            let Some(path) = parts.next() else {
                continue;
            };
            let Some(kind) = parts.next() else {
                continue;
            };
            if kind == "part" {
                candidates.push(PathBuf::from(path));
            }
        }
    }

    if candidates.is_empty()
        && let Some(name) = loop_device.file_name().and_then(|value| value.to_str())
    {
        let sys_block = Path::new("/sys/block").join(name);
        if let Ok(entries) = fs::read_dir(sys_block) {
            for entry in entries.flatten() {
                let partition_name = entry.file_name();
                let partition_name = partition_name.to_string_lossy();
                if partition_name.starts_with(name) && partition_name != name {
                    candidates.push(Path::new("/dev").join(partition_name.as_ref()));
                }
            }
        }
    }

    candidates.push(loop_device.to_path_buf());
    candidates
}

#[cfg(target_os = "linux")]
fn unmount_image(mount_dir: &Path, loop_device: Option<&Path>) -> Result<(), String> {
    let output = Command::new("umount")
        .arg(mount_dir)
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(command_error_message(&output, "unmount failed"));
    }
    if let Some(loop_device) = loop_device {
        let output = Command::new("losetup")
            .arg("-d")
            .arg(loop_device)
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err(command_error_message(&output, "loop device detach failed"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn unmount_image(_mount_dir: &Path, _loop_device: Option<&Path>) -> Result<(), String> {
    Err(
        "Windows mount helper unmount requires the image path and is handled by the UI process"
            .to_string(),
    )
}

#[cfg(not(any(target_os = "linux", windows)))]
fn unmount_image(_mount_dir: &Path, _loop_device: Option<&Path>) -> Result<(), String> {
    Err("image unmount helper is not supported on this platform".to_string())
}

fn command_error_message(output: &std::process::Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        fallback.to_string()
    } else {
        stderr
    }
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

/// CLI komutları için varsayılan vaka klasörünü oluşturur.
fn cli_case_vault(case_name: &str) -> Result<EvidenceVault, String> {
    let clean = amele::api::sanitize_case_name(case_name);
    EvidenceVault::create(amele::api::default_case_base_dir(), clean)
        .map_err(|err| crate_diagnostic(err.to_string()))
}

/// Dosya adlarında güvenli kısa parça üretir.
fn cli_safe_stem(value: &str) -> String {
    let clean = value
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
        .to_string();
    if clean.is_empty() {
        "output".to_string()
    } else {
        clean
    }
}

/// CLI dosya adları için ortak zaman damgası üretir.
fn cli_timestamp() -> String {
    Local::now().format("%Y%m%d_%H%M%S").to_string()
}

/// CLI argümanlarından raw/aff4 format seçimini ayıklar.
fn extract_output_format(args: &mut Vec<String>) -> Result<AcquisitionOutputFormat, String> {
    let mut selected = None;
    args.retain(|arg| {
        let parsed = if let Some(format) = arg.strip_prefix("--format=") {
            Some(format)
        } else if arg == "--aff4" {
            Some("aff4")
        } else if arg == "--raw" {
            Some("raw")
        } else if matches!(arg.as_str(), "raw" | "dd" | "img" | "aff4") {
            Some(arg.as_str())
        } else {
            None
        };

        if let Some(fmt) = parsed {
            selected = Some(fmt.to_string());
            false
        } else {
            true
        }
    });
    AcquisitionOutputFormat::parse(selected.as_deref())
}

/// Global --profile argümanını komut listesinden ayırır.
fn extract_global_profile(args: &mut Vec<String>) -> Option<String> {
    if let Some(pos) = args
        .iter()
        .position(|a| a == "--profile" || a.starts_with("--profile="))
    {
        let arg = args.remove(pos);
        if let Some(val) = arg.strip_prefix("--profile=") {
            Some(val.to_string())
        } else if pos < args.len() {
            Some(args.remove(pos))
        } else {
            None
        }
    } else {
        None
    }
}

/// Global --en bayrağını komut listesinden ayıklar.
fn extract_global_en_flag(args: &mut Vec<String>) -> bool {
    if let Some(pos) = args.iter().position(|a| a == "--en") {
        args.remove(pos);
        true
    } else {
        false
    }
}

/// Profil komutlarında otomatik açılış bayrağını ayıklar.
fn remove_direct_flag(args: &mut Vec<String>) -> bool {
    let original_len = args.len();
    args.retain(|arg| arg != "--direct" && arg != "--open-directly");
    args.len() != original_len
}

/// JSON çıktıyı stdout'a pretty formatta yazar.
fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|err| err.to_string())?
    );
    Ok(())
}

/// Byte bazlı ilerlemeyi stderr'e yüzde olarak yazar.
fn print_progress(label: &str, done: u64, total: u64) {
    if total == 0 {
        eprintln!("{label}: {done} byte");
    } else {
        let percent = done.saturating_mul(100).checked_div(total).unwrap_or(0);
        eprintln!("{label}: {percent}% [{done}/{total}]");
    }
}

/// Adım bazlı ilerlemeyi stderr'e yazar.
fn print_step_progress(label: &str, done: u32, total: u32, step: &str) {
    if total == 0 {
        eprintln!("{label}: {step}");
    } else {
        let shown = if done == 0 { 1 } else { done.min(total) };
        eprintln!("{label}: {}/{} {}", shown, total, step);
    }
}

/// CLI hatalarını uygulamanın zengin hata açıklamasıyla döndürür.
fn crate_diagnostic(message: String) -> String {
    amele::diagnostics::error_with_advice(&message)
}

fn disk_size_command(args: Vec<String>) -> Result<(), String> {
    let Some(path) = args.first() else {
        return Err("Kullanim: disk-size <cihaz|dosya>".to_string());
    };
    let size = disk::disk_size(path).map_err(|err| err.to_string())?;
    println!("{size}");
    Ok(())
}

fn verify_command(args: Vec<String>) -> Result<(), String> {
    if args.len() != 2 {
        return Err(t_cli(
            "Kullanim: verify <imaj> <sha256>",
            "Usage: verify <image> <sha256>",
        ));
    }
    let ok = disk::verify_image(&args[0], &args[1]).map_err(|err| err.to_string())?;
    println!("{}", if ok { "OK" } else { "FAIL" });
    Ok(())
}

fn remote_disks_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(t_cli(
            "Kullanim: remote-disks <ip> <port> [token]",
            "Usage: remote-disks <ip> <port> [token]",
        ));
    }
    let port = parse_port(&args[1])?;
    let token = args.get(2).cloned();
    let mut connection =
        RemoteConnection::connect(&args[0], port, token).map_err(|err| err.to_string())?;
    let disks = connection.list_disks().map_err(|err| err.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&disks).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn remote_image_command(args: Vec<String>) -> Result<(), String> {
    let mut args = args;
    let selected_format = extract_output_format(&mut args)?;
    if args.len() < 4 {
        return Err(t_cli(
            "Kullanim: remote-image <ip> <port> <disk_id> <cikti_klasoru> [token] [raw|aff4]",
            "Usage: remote-image <ip> <port> <disk_id> <out_dir> [token] [raw|aff4]",
        ));
    }
    let port = parse_port(&args[1])?;
    let token = args.get(4).cloned();
    let mut connection =
        RemoteConnection::connect(&args[0], port, token).map_err(|err| err.to_string())?;
    let result = connection
        .acquire_image(
            &args[2],
            None,
            &args[3],
            None,
            selected_format,
            |done: u64, total: u64| {
                if let Some(percent) = done.saturating_mul(100).checked_div(total) {
                    eprintln!("{}%", percent);
                }
            },
        )
        .map_err(|err| err.to_string())?;
    let plan = output_format::OutputPlan {
        format: selected_format,
        working_path: result.target_path.clone(),
        final_path: result.target_path.with_extension("aff4"),
    };
    let finalized =
        output_format::finalize_output(&plan, "disk", &args[2], "", result.sha256.clone())?;
    print_json(&json!({
        "remote_job_id": result.job_id,
        "target_path": finalized.target_path,
        "bytes_transferred": result.bytes_transferred,
        "sha256": finalized.sha256,
        "raw_sha256": finalized.raw_sha256,
        "output_format": finalized.format.as_str(),
        "md5": result.md5,
        "message": result.message,
    }))
}

fn remote_tool_check_command(args: Vec<String>) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Kullanim: remote-tool-check <ip> <port> <winpmem|avml> [token]".to_string());
    }
    let port = parse_port(&args[1])?;
    let token = args.get(3).cloned();
    let mut connection =
        RemoteConnection::connect(&args[0], port, token).map_err(|err| err.to_string())?;
    let status = match args[2].as_str() {
        "winpmem" => connection.check_winpmem(),
        "avml" => connection.check_avml(),
        other => return Err(format!("Bilinmeyen arac: {other}")),
    }
    .map_err(|err| err.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&status).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn ram_status_command() -> Result<(), String> {
    let status = serde_json::json!({
        "avml": ram::avml_status(None),
        "winpmem": ram::winpmem_status(None),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&status).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn wireguard_config_command(args: Vec<String>) -> Result<(), String> {
    let Some(path) = args.first() else {
        return Err("Kullanim: wireguard-config <dosya>".to_string());
    };
    let written = wireguard::create_config(path, &WireGuardConfig::default())
        .map_err(|err| err.to_string())?;
    println!("{}", written.display());
    Ok(())
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| "Port 1 ile 65535 arasinda olmali".to_string())
        .and_then(|port| {
            if port == 0 {
                Err("Port 1 ile 65535 arasinda olmali".to_string())
            } else {
                Ok(port)
            }
        })
}

fn update_check_command(args: Vec<String>) -> Result<(), String> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let response = api::update::update_check_endpoint();
    if response.status != 200 {
        let err_msg = String::from_utf8_lossy(&response.body);
        return Err(format!("Güncelleme kontrolü başarısız oldu: {err_msg}"));
    }
    let data: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|e| format!("Yanıt çözümlenemedi: {e}"))?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&data).unwrap());
        return Ok(());
    }

    let current = data
        .get("current_version")
        .and_then(|v| v.as_str())
        .unwrap_or("v0.0.0");
    let latest_tag = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("bilinmiyor");
    let release_name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(latest_tag);
    let html_url = data.get("html_url").and_then(|v| v.as_str()).unwrap_or("");

    let target = data.get("update_target");
    let pkg_label = target
        .and_then(|t| t.get("package_label"))
        .and_then(|v| v.as_str())
        .unwrap_or("Bilinmeyen");
    let detected_by = target
        .and_then(|t| t.get("detected_by"))
        .and_then(|v| v.as_str())
        .unwrap_or("sistem");
    let install_cmd = target
        .and_then(|t| t.get("install_command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let asset = data.get("platform_asset");
    let asset_name = asset
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("bulunamadı");
    let asset_url = asset
        .and_then(|a| a.get("download_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let asset_size = asset
        .and_then(|a| a.get("size"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let clean_current = current.trim_start_matches('v');
    let clean_latest = latest_tag.trim_start_matches('v');
    let has_update = clean_latest != clean_current && !clean_latest.is_empty();

    println!("=== Amele Forensic Tool Güncelleme Kontrolü ===");
    println!("Mevcut Sürüm   : v{}", clean_current);
    println!(
        "Son Sürüm      : {}",
        if latest_tag.starts_with('v') {
            latest_tag.to_string()
        } else {
            format!("v{}", latest_tag)
        }
    );
    println!("Paket Türü     : {} (Algılama: {})", pkg_label, detected_by);

    if has_update {
        println!("\n[!] YENİ SÜRÜM MEVCUT! ({})", release_name);
        println!("İndirilecek Dosya : {}", asset_name);
        if asset_size > 0 {
            println!(
                "Dosya Boyutu      : {:.2} MB",
                asset_size as f64 / 1_048_576.0
            );
        }
        if !asset_url.is_empty() {
            println!("İndirme Bağlantısı : {}", asset_url);
        }
        if !html_url.is_empty() {
            println!("Sürüm Notları     : {}", html_url);
        }
        if !install_cmd.is_empty() {
            println!("\nÖnerilen Kurulum Komutu:\n  {}", install_cmd);
        }
    } else {
        println!("\n[✓] Sisteminiz güncel. En son sürümü kullanıyorsunuz.");
    }

    Ok(())
}

fn preflight_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanım: amele preflight <kaynak_yol> <kaynak_tipi> [hedef_yol]\nKaynak tipleri: disk, ram, android, ios".to_string());
    }
    let source_path = &args[0];
    let source_type = args.get(1).map(|s| s.as_str()).unwrap_or("disk");
    let target_path = args
        .get(2)
        .map(|s| std::path::PathBuf::from(s))
        .unwrap_or_else(|| amele::api::default_case_base_dir());

    let result = amele::storage_guard::preflight_check(source_path, source_type, &target_path);
    println!("=== Disk Alanı Ön Kontrolü ===");
    println!("Kaynak       : {}", source_path);
    println!("Kaynak Tipi  : {}", source_type);
    println!(
        "Kaynak Boyutu: {:.2} GB",
        result.source_bytes as f64 / 1_073_741_824.0
    );
    println!(
        "Boş Alan     : {:.2} GB",
        result.available_bytes as f64 / 1_073_741_824.0
    );
    if result.is_sufficient {
        println!("\n✓ Yeterli disk alanı mevcut.");
    } else {
        println!("\n✗ YETERSİZ ALAN!");
        println!(
            "Eksik        : {:.2} GB",
            result.shortage_bytes as f64 / 1_073_741_824.0
        );
        if let Some(msg) = &result.warning_message {
            println!("Uyarı        : {}", msg);
        }
    }
    Ok(())
}

fn mount_list_command() -> Result<(), String> {
    let mounts = amele::mount_tracker::list_active_mounts();
    if mounts.is_empty() {
        println!("Aktif bağlı imaj bulunmuyor.");
        return Ok(());
    }
    println!("=== Aktif Bağlı İmajlar ===");
    for m in &mounts {
        println!("  ID          : {}", m.mount_id);
        println!("  Vaka        : {}", m.case_name);
        println!("  İmaj        : {}", m.image_path.display());
        println!("  Bağlama     : {}", m.mount_point.display());
        println!("  Tarih       : {}", m.mounted_at);
        println!("  ---");
    }
    println!("Toplam: {} adet", mounts.len());
    Ok(())
}

fn mount_cleanup_command(args: Vec<String>) -> Result<(), String> {
    let case_filter = args.first().map(|s| s.as_str());
    let cleaned = if let Some(case) = case_filter {
        println!("Vaka '{}' için mount temizliği yapılıyor...", case);
        amele::mount_tracker::cleanup_case_mounts(case)
    } else {
        println!("Tüm aktif mount'lar temizleniyor...");
        amele::mount_tracker::cleanup_all_mounts()
    };
    if cleaned.is_empty() {
        println!("Temizlenecek mount bulunamadı.");
    } else {
        for id in &cleaned {
            println!("  ✓ Temizlendi: {}", id);
        }
        println!("Toplam {} mount temizlendi.", cleaned.len());
    }
    Ok(())
}

fn case_export_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanım: amele case-export <vaka_adi> [hedef_dosya]".to_string());
    }
    let case_name = &args[0];
    let base_dir = amele::api::default_case_base_dir();
    let vault =
        amele::evidence::EvidenceVault::create(&base_dir, case_name).map_err(|e| e.to_string())?;

    let output_path = if let Some(p) = args.get(1) {
        std::path::PathBuf::from(p)
    } else {
        base_dir.clone()
    };

    println!("Vaka dışa aktarılıyor: {}", case_name);
    let package_path =
        amele::case_package::export_case(&vault, &output_path).map_err(|e| e.to_string())?;
    println!("✓ Paket oluşturuldu: {}", package_path.display());

    let hash_path = package_path.with_extension(format!(
        "{}sha256",
        package_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default()
    ));
    if hash_path.is_file() {
        println!("✓ Hash dosyası : {}", hash_path.display());
    }
    Ok(())
}

fn case_import_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanım: amele case-import <amelecase_dosyasi>".to_string());
    }
    let package_path = std::path::Path::new(&args[0]);
    if !package_path.is_file() {
        return Err(format!("Dosya bulunamadı: {}", package_path.display()));
    }

    let base_dir = amele::api::default_case_base_dir();
    println!("Vaka içe aktarılıyor: {}", package_path.display());

    let result =
        amele::case_package::import_case(package_path, &base_dir).map_err(|e| e.to_string())?;

    println!("✓ Vaka aktarıldı    : {}", result.case_name);
    println!("  Hedef klasör      : {}", result.case_dir.display());
    println!("  Dosya sayısı      : {}", result.files_extracted);
    println!(
        "  Bütünlük doğrulandı: {}",
        if result.integrity_verified {
            "Evet ✓"
        } else {
            "Hayır ✗"
        }
    );
    for warn in &result.warnings {
        println!("  ⚠ {}", warn);
    }
    Ok(())
}

fn case_verify_command(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err("Kullanım: amele case-verify <amelecase_dosyasi>".to_string());
    }
    let package_path = std::path::Path::new(&args[0]);
    if !package_path.is_file() {
        return Err(format!("Dosya bulunamadı: {}", package_path.display()));
    }

    println!("Paket bütünlüğü doğrulanıyor: {}", package_path.display());
    let verified = amele::case_package::verify_package(package_path).map_err(|e| e.to_string())?;

    if verified {
        println!("✓ Paket bütünlüğü doğrulandı. SHA-256 hash eşleşiyor.");
    } else {
        println!("✗ UYARI: Paket bütünlüğü doğrulanamadı! SHA-256 hash eşleşmiyor.");
    }
    Ok(())
}
