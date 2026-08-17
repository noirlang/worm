//! HTTP metod ve path değerlerini ilgili API endpoint fonksiyonlarına bağlar.
use crate::server::{Response, json_error, json_ok};

use super::{
    acquisition_control, android, desktop, developer, docker, evidence, hash_api, ios, profile,
    ram, settings, ssh, storage, system, update, wireguard,
};

/// API HTTP metod/path çiftini ilgili endpoint fonksiyonuna yönlendirir ve detaylıca loglar.
pub fn route_api(method: &str, path: &str, body: &[u8]) -> Response {
    if path != "/api/acquisition-status" && path != "/api/developer-logs" {
        let body_str = if body.is_empty() {
            "(boş)".to_string()
        } else if body.len() > 250 {
            format!(
                "{}... (toplam {} byte)",
                String::from_utf8_lossy(&body[..250]).trim(),
                body.len()
            )
        } else {
            String::from_utf8_lossy(body).trim().to_string()
        };

        crate::logging::runtime_log(
            crate::logging::LogLevel::Debug,
            "api:router",
            format!(
                "API ISTEK BASLADI | {} {} | Parametreler: {}",
                method, path, body_str
            ),
        );
    }

    let response = match (method, path) {
        ("GET", "/api/health") => json_ok(serde_json::json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
        })),
        ("GET", "/api/developer-logs") => {
            // Polling istekleri konsolu kirletmesin diye log seviyesini düşürebilir veya loglamayabiliriz.
            developer::developer_logs_endpoint()
        }
        ("POST", "/api/developer-log") => developer::developer_log_endpoint(body),
        ("POST", "/api/open-dev-console") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:devconsole",
                "Developer konsol penceresi aciliyor",
            );
            desktop::open_dev_console_endpoint()
        }
        ("GET", "/api/settings") => settings::settings_get_endpoint(),
        ("POST", "/api/settings") => settings::settings_save_endpoint(body),
        ("GET", "/api/profiles") => profile::profiles_get_endpoint(),
        ("POST", "/api/profiles/create") => profile::profile_create_endpoint(body),
        ("POST", "/api/profiles/select") => profile::profile_select_endpoint(body),
        ("POST", "/api/profiles/logout") => profile::profile_logout_endpoint(),
        ("POST", "/api/profiles/online-login") => profile::profile_online_login_endpoint(body),
        ("POST", "/api/profiles/online-sync") => profile::profile_online_sync_endpoint(),
        ("POST", "/api/profiles/online-logout") => profile::profile_online_logout_endpoint(),
        ("GET", "/api/profiles/mobile-access") => profile::profile_mobile_access_endpoint(),
        ("GET", "/api/settings-default") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Debug,
                "api:settings",
                "Varsayilan ayarlar talep edildi",
            );
            match serde_json::to_value(crate::settings::AppSettings::default()) {
                Ok(value) => json_ok(value),
                Err(err) => {
                    crate::logging::runtime_log(
                        crate::logging::LogLevel::Error,
                        "api:settings",
                        format!("Varsayilan ayarlar serilestirilemedi: {}", err),
                    );
                    json_error(500, err.to_string())
                }
            }
        }
        ("GET", "/api/disk-list") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:disk",
                "Sistem disk listesi talep edildi",
            );
            system::disk_list_endpoint()
        }
        ("GET", "/api/android-adb-status") => {
            if let Some(response) = profile::require_mobile_tools_response() {
                response
            } else {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Info,
                    "api:android",
                    "ADB servis durumu sorgulaniyor",
                );
                match serde_json::to_value(crate::android::adb_status()) {
                    Ok(value) => json_ok(value),
                    Err(err) => {
                        crate::logging::runtime_log(
                            crate::logging::LogLevel::Error,
                            "api:android",
                            format!("ADB durum bilgisi donulemedi: {}", err),
                        );
                        json_error(500, err.to_string())
                    }
                }
            }
        }
        ("POST", "/api/android-adb-install") => {
            if let Some(response) = profile::require_mobile_tools_response() {
                response
            } else {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Info,
                    "api:android",
                    "ADB otomatik kurulumu baslatiliyor",
                );
                match crate::android::install_adb() {
                    Ok(result) => match serde_json::to_value(result) {
                        Ok(value) => json_ok(value),
                        Err(err) => json_error(500, err.to_string()),
                    },
                    Err(err) => json_error(500, crate::android::explain_android_error(err)),
                }
            }
        }
        ("GET", "/api/android-devices") => {
            if let Some(response) = profile::require_mobile_tools_response() {
                response
            } else {
                crate::logging::runtime_log(
                    crate::logging::LogLevel::Info,
                    "api:android",
                    "Bagli Android cihaz listesi talep edildi",
                );
                match crate::android::list_devices() {
                    Ok(devices) => json_ok(serde_json::json!({ "devices": devices })),
                    Err(err) => {
                        let err_msg = crate::android::explain_android_error(err);
                        crate::logging::runtime_log(
                            crate::logging::LogLevel::Error,
                            "api:android",
                            format!("Android cihazlari listelenemedi: {}", err_msg),
                        );
                        json_error(500, err_msg)
                    }
                }
            }
        }
        ("POST", "/api/android-device-profile") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Cihaz profil bilgisi sorgulaniyor",
            );
            android::android_device_profile_endpoint(body)
        }
        ("POST", "/api/android-profile-acquisition") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Android profil edinim isi baslatiliyor",
            );
            android::android_profile_acquisition_endpoint(body)
        }
        ("POST", "/api/android-logical-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Android mantiksal edinim isi baslatiliyor",
            );
            android::android_logical_image_endpoint(body)
        }
        ("POST", "/api/android-filesystem-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Android dosya sistemi imaj isi baslatiliyor",
            );
            android::android_filesystem_image_endpoint(body)
        }
        ("POST", "/api/android-ram-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Android RAM/Volatile edinim isi baslatiliyor",
            );
            android::android_ram_image_endpoint(body)
        }
        ("POST", "/api/android-lemon-preflight") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Lemon preflight RAM analizi baslatiliyor",
            );
            android::android_lemon_preflight_endpoint(body)
        }
        ("POST", "/api/android-remote-connect") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Uzak ADB/MESH baglantisi baslatiliyor",
            );
            android::android_remote_connect_endpoint(body)
        }
        ("POST", "/api/android-remote-disconnect") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Uzak ADB/MESH baglantisi kesiliyor",
            );
            android::android_remote_disconnect_endpoint(body)
        }
        ("POST", "/api/android-case-analysis") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:android",
                "Android vaka analiz ozeti talep edildi",
            );
            android::android_case_analysis_endpoint(body)
        }
        ("POST", "/api/ios-backup-profile") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ios",
                "iOS backup profil bilgisi sorgulaniyor",
            );
            ios::ios_backup_profile_endpoint(body)
        }
        ("POST", "/api/ios-backup-normalize") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ios",
                "iOS backup normalizasyon isi baslatiliyor",
            );
            ios::ios_backup_normalize_endpoint(body)
        }
        ("GET", "/api/ram-status") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM edinim araclari (AVML/WinPMEM) durum sorgusu",
            );
            json_ok(serde_json::json!({
                "avml": crate::ram::avml_status(None),
                "winpmem": crate::ram::winpmem_status(None),
            }))
        }
        ("POST", "/api/avml-install") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "AVML kurulum istegi alindi",
            );
            ram::avml_install_endpoint()
        }
        ("POST", "/api/winpmem-install") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "WinPMEM kurulum istegi alindi",
            );
            ram::winpmem_install_endpoint()
        }
        ("POST", "/api/acquisition-control") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:job",
                "Edinim isi kontrol komutu alindi",
            );
            acquisition_control::acquisition_control_endpoint(body)
        }
        ("POST", "/api/acquisition-status") => {
            acquisition_control::acquisition_status_endpoint(body)
        }
        ("POST", "/api/connect") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:remote",
                "Uzak ajana baglanti denemesi",
            );
            system::connect_endpoint(body)
        }
        ("POST", "/api/hash") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:hash",
                "Dosya hash hesaplama baslatildi",
            );
            hash_api::hash_endpoint(body)
        }
        ("POST", "/api/local-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:image",
                "Yerel disk imaj alma isi baslatiliyor",
            );
            system::local_image_endpoint(body)
        }
        ("POST", "/api/local-ram") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "Yerel RAM edinim isi baslatiliyor",
            );
            ram::local_ram_endpoint(body)
        }
        ("POST", "/api/remote-disks") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:remote",
                "Uzak ajan disk listesi sorgulaniyor",
            );
            system::remote_disks_endpoint(body)
        }
        ("POST", "/api/remote-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:remote",
                "Uzak ajan disk imaj alma isi baslatiliyor",
            );
            system::remote_image_endpoint(body)
        }
        ("POST", "/api/remote-ram") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:remote",
                "Uzak ajan RAM edinim isi baslatiliyor",
            );
            ram::remote_ram_endpoint(body)
        }
        ("POST", "/api/remote-tool-check") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:remote",
                "Uzak ajan arac kontrolu yapiliyor",
            );
            system::remote_tool_check_endpoint(body)
        }
        ("POST", "/api/ssh-connect") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ssh",
                "SSH sunucu baglanti testi yapiliyor",
            );
            ssh::ssh_connect_endpoint(body)
        }
        ("POST", "/api/ssh-disks") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ssh",
                "SSH ile uzak disk listesi sorgulaniyor",
            );
            ssh::ssh_disks_endpoint(body)
        }
        ("POST", "/api/ssh-tool-check") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ssh",
                "SSH ile RAM araclari kontrol ediliyor",
            );
            ssh::ssh_tool_check_endpoint(body)
        }
        ("POST", "/api/ssh-image") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ssh",
                "SSH ile uzak disk imaj alma isi baslatiliyor",
            );
            ssh::ssh_image_endpoint(body)
        }
        ("POST", "/api/ssh-ram") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ssh",
                "SSH ile uzak RAM edinim isi baslatiliyor",
            );
            ssh::ssh_ram_endpoint(body)
        }
        ("POST", "/api/evidence-create") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:evidence",
                "Yeni adli vaka olusturuluyor",
            );
            evidence::evidence_create_endpoint(body)
        }
        ("POST", "/api/evidence-add-note") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:evidence",
                "Vakaya not ekleniyor",
            );
            evidence::evidence_add_note_endpoint(body)
        }
        ("POST", "/api/evidence-list-files") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:evidence",
                "Vaka dosyalari listeleniyor",
            );
            evidence::evidence_list_files_endpoint(body)
        }
        ("POST", "/api/evidence-manifest") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:evidence",
                "Vaka butunluk manifesti uretiliyor",
            );
            evidence::evidence_manifest_endpoint(body)
        }
        ("POST", "/api/acquisition-history") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:evidence",
                "Android/iOS edinim gecmisi listeleniyor",
            );
            evidence::acquisition_history_endpoint(body)
        }
        ("GET", "/api/evidence-cases") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Debug,
                "api:evidence",
                "Tüm vakalar listeleniyor",
            );
            evidence::evidence_cases_endpoint()
        }
        ("GET", "/api/evidence-summary") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Debug,
                "api:evidence",
                "Adli vaka ozeti talep edildi",
            );
            evidence::evidence_summary_endpoint()
        }
        ("POST", "/api/report-create") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:report",
                "Rapor olusturma baslatildi",
            );
            evidence::report_create_endpoint(body)
        }
        ("POST", "/api/image-mount-readonly") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:image",
                "Imaj salt-okunur baglaniyor",
            );
            system::image_mount_readonly_endpoint(body)
        }
        ("POST", "/api/image-unmount") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:image",
                "Imaj baglantisi kesiliyor",
            );
            system::image_unmount_endpoint()
        }
        ("POST", "/api/image-analyze") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:image",
                "Imaj analizi baslatildi",
            );
            system::image_analyze_endpoint(body)
        }
        ("POST", "/api/image-browse") => system::image_browse_endpoint(body),
        ("POST", "/api/image-read-file") => system::image_read_file_endpoint(body),
        ("POST", "/api/ram-analyze-summary") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM analiz ozeti sorgulaniyor",
            );
            ram::ram_analyze_summary_endpoint(body)
        }
        ("POST", "/api/ram-volatility-preflight") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "Volatility preflight sembol kontrolü",
            );
            ram::ram_volatility_preflight_endpoint(body)
        }
        ("POST", "/api/ram-volatility-symbol-install") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "Volatility sembol yukleme baslatildi",
            );
            ram::ram_volatility_symbol_install_endpoint(body)
        }
        ("POST", "/api/ram-analyze-summary-start") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "Volatility analiz ozeti isi baslatiliyor",
            );
            ram::ram_analyze_summary_start_endpoint(body)
        }
        ("POST", "/api/ram-analyze-strings") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM strings analizi yapiliyor",
            );
            ram::ram_analyze_strings_endpoint(body)
        }
        ("POST", "/api/ram-carve-files") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM dosya carve islemi baslatildi",
            );
            ram::ram_carve_files_endpoint(body)
        }
        ("POST", "/api/ram-list-processes") => ram::ram_list_processes_endpoint(body),
        ("POST", "/api/ram-list-processes-start") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM Volatility surec listeleme isi baslatiliyor",
            );
            ram::ram_list_processes_start_endpoint(body)
        }
        ("POST", "/api/ram-process-details") => ram::ram_process_details_endpoint(body),
        ("POST", "/api/ram-process-details-start") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:ram",
                "RAM Volatility surec detay isi baslatiliyor",
            );
            ram::ram_process_details_start_endpoint(body)
        }
        ("POST", "/api/ram-process-search") => ram::ram_process_search_endpoint(body),
        ("POST", "/api/ram-read-carved") => ram::ram_read_carved_endpoint(body),
        ("POST", "/api/wireguard-config") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:vpn",
                "Wireguard VPN konfigürasyonu olusturuluyor",
            );
            wireguard::wireguard_config_endpoint(body)
        }
        ("POST", "/api/wireguard-start") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:vpn",
                "Wireguard VPN tüneli baslatiliyor",
            );
            wireguard::wireguard_start_endpoint(body)
        }
        ("POST", "/api/wireguard-stop") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:vpn",
                "Wireguard VPN tüneli durduruluyor",
            );
            wireguard::wireguard_stop_endpoint()
        }
        ("GET", "/api/wireguard-status") => wireguard::wireguard_status_endpoint(),
        ("GET", "/api/update-check") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:system",
                "Uygulama guncelleme kontrolü yapiliyor",
            );
            update::update_check_endpoint()
        }
        ("GET", "/api/update-target") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:system",
                "Lokal guncelleme paket tipi algilaniyor",
            );
            update::update_target_endpoint()
        }
        ("POST", "/api/update-download") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:system",
                "Guncelleme paketi indiriliyor",
            );
            update::update_download_endpoint(body)
        }
        ("POST", "/api/update-install") => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Info,
                "api:system",
                "Guncelleme paketi kuruluyor",
            );
            update::update_install_endpoint(body)
        }
        ("GET", "/api/docker-status") => docker::docker_status_endpoint(None),
        ("POST", "/api/docker-status") => {
            let custom_root = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("custom_docker_root")
                        .and_then(|s| s.as_str())
                        .map(str::to_string)
                });
            docker::docker_status_endpoint(custom_root.as_deref())
        }
        ("GET", "/api/docker-containers") => docker::docker_containers_endpoint(None),
        ("POST", "/api/docker-containers") => {
            let custom_root = serde_json::from_slice::<serde_json::Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("custom_docker_root")
                        .and_then(|s| s.as_str())
                        .map(str::to_string)
                });
            docker::docker_containers_endpoint(custom_root.as_deref())
        }
        ("POST", "/api/docker-logs") => docker::docker_logs_endpoint(body),
        ("POST", "/api/docker-acquire-local") => docker::docker_acquire_local_endpoint(body),
        ("POST", "/api/docker-remote-status") => docker::docker_remote_status_endpoint(body),
        ("POST", "/api/docker-remote-containers") => {
            docker::docker_remote_containers_endpoint(body)
        }
        ("POST", "/api/docker-remote-logs") => docker::docker_remote_logs_endpoint(body),
        ("POST", "/api/docker-remote-acquire") => docker::docker_remote_acquire_endpoint(body),
        ("POST", "/api/open-url") => desktop::open_url_endpoint(body),
        ("POST", "/api/pick-file") => desktop::pick_path_endpoint(false),
        ("POST", "/api/pick-folder") => desktop::pick_path_endpoint(true),
        ("POST", "/api/preflight-storage-check") => storage::preflight_storage_check_endpoint(body),
        ("GET", "/api/mounts-active") => storage::active_mounts_endpoint(),
        ("POST", "/api/mounts-cleanup") => storage::cleanup_mounts_endpoint(body),
        _ => {
            crate::logging::runtime_log(
                crate::logging::LogLevel::Warn,
                "api:router",
                format!("Bilinmeyen API rotasi cagirildi: {} {}", method, path),
            );
            json_error(404, "api endpoint not found")
        }
    };

    let status = response.status;
    crate::logging::runtime_log(
        crate::logging::LogLevel::Debug,
        "api:router",
        format!(
            "API ISTEK TAMAMLANDI | {} {} → HTTP {}",
            method, path, status
        ),
    );

    response
}
