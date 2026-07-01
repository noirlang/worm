//! Android mantıksal edinim profillerini ve çalıştırılacak adımları tanımlar.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Android mantıksal edinimde kullanılacak kapsam profilini belirtir.
pub enum AndroidAcquisitionProfile {
    QuickLogical,
    FullLogical,
    RootLogical,
    Volatile,
}

impl AndroidAcquisitionProfile {
    /// UI veya API'den gelen profil kimliğini güvenli varsayılanla enum değerine çevirir.
    pub fn from_id(value: &str) -> Self {
        match value.trim() {
            "quick_logical" | "quick" => Self::QuickLogical,
            "root_logical" | "root" => Self::RootLogical,
            "volatile" | "ram" => Self::Volatile,
            _ => Self::FullLogical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Tek bir Android toplama adımının kategori ve çıktı dosyası eşleşmesini tutar.
pub struct AndroidExtractorStep {
    pub category: &'static str,
    pub file_name: &'static str,
}

impl AndroidExtractorStep {
    /// Sabit edinim adımı tanımlar.
    pub const fn new(category: &'static str, file_name: &'static str) -> Self {
        Self {
            category,
            file_name,
        }
    }
}

const QUICK_LOGICAL: &[&str] = &[
    "device_info",
    "packages",
    "packages_json",
    "social_apps",
    "turkey_app_storage",
    "dumpsys_usagestats",
    "dumpsys_account",
    "social_accounts",
    "dumpsys_connectivity",
    "dumpsys_notification",
    "dumpsys_telephony",
    "dumpsys_location",
    "processes",
    "content_sms",
    "content_calls",
    "content_contacts",
    "content_media_images",
    "content_media_videos",
    "social_processes",
    "bugreport",
];

const ROOT_LOGICAL_EXTRA: &[&str] = &[
    "root_status",
    "root_binaries",
    "selinux_status",
    "mounts",
    "procfs_summary",
    "proc_memory_maps",
    "heapdump_candidates",
    "debug_heap_dumps",
    "dumpsys_keystore",
];

const VOLATILE_ONLY: &[&str] = &[
    "root_status",
    "procfs_summary",
    "proc_memory_maps",
    "dumpsys_meminfo",
    "dumpsys_procstats",
    "heapdump_candidates",
    "debug_heap_dumps",
    "logcat",
];

pub const FULL_LOGICAL_STEPS: &[AndroidExtractorStep] = &[
    AndroidExtractorStep::new("device_info", "device_info.txt"),
    AndroidExtractorStep::new("packages", "packages.txt"),
    AndroidExtractorStep::new("packages_json", "packages.json"),
    AndroidExtractorStep::new("social_apps", "social_apps.json"),
    AndroidExtractorStep::new("turkey_app_storage", "turkey_app_storage.json"),
    AndroidExtractorStep::new("logcat", "logcat.txt"),
    AndroidExtractorStep::new("system_logs", "system_logs.txt"),
    AndroidExtractorStep::new("dumpsys_battery", "dumpsys_battery.txt"),
    AndroidExtractorStep::new("dumpsys_wifi", "dumpsys_wifi.txt"),
    AndroidExtractorStep::new("dumpsys_bluetooth", "dumpsys_bluetooth.txt"),
    AndroidExtractorStep::new("dumpsys_usagestats", "dumpsys_usagestats.txt"),
    AndroidExtractorStep::new("dumpsys_account", "dumpsys_account.txt"),
    AndroidExtractorStep::new("social_accounts", "social_accounts.json"),
    AndroidExtractorStep::new("dumpsys_connectivity", "dumpsys_connectivity.txt"),
    AndroidExtractorStep::new("dumpsys_notification", "dumpsys_notification.txt"),
    AndroidExtractorStep::new("dumpsys_telephony", "dumpsys_telephony.txt"),
    AndroidExtractorStep::new("dumpsys_location", "dumpsys_location.txt"),
    AndroidExtractorStep::new("dumpsys_netstats", "dumpsys_netstats.txt"),
    AndroidExtractorStep::new("dumpsys_activity", "dumpsys_activity.txt"),
    AndroidExtractorStep::new("dumpsys_meminfo", "dumpsys_meminfo.txt"),
    AndroidExtractorStep::new("dumpsys_appops", "dumpsys_appops.txt"),
    AndroidExtractorStep::new("dumpsys_package", "dumpsys_package.txt"),
    AndroidExtractorStep::new("dumpsys_diskstats", "dumpsys_diskstats.txt"),
    AndroidExtractorStep::new("dumpsys_deviceidle", "dumpsys_deviceidle.txt"),
    AndroidExtractorStep::new("dumpsys_alarm", "dumpsys_alarm.txt"),
    AndroidExtractorStep::new("dumpsys_jobscheduler", "dumpsys_jobscheduler.txt"),
    AndroidExtractorStep::new("dumpsys_procstats", "dumpsys_procstats.txt"),
    AndroidExtractorStep::new("dumpsys_sensorservice", "dumpsys_sensorservice.txt"),
    AndroidExtractorStep::new("dumpsys_power", "dumpsys_power.txt"),
    AndroidExtractorStep::new("dumpsys_window", "dumpsys_window.txt"),
    AndroidExtractorStep::new("dumpsys_clipboard", "dumpsys_clipboard.txt"),
    AndroidExtractorStep::new("dumpsys_batterystats", "dumpsys_batterystats.txt"),
    AndroidExtractorStep::new("dumpsys_keystore", "dumpsys_keystore.txt"),
    AndroidExtractorStep::new("root_status", "root_status.txt"),
    AndroidExtractorStep::new("root_binaries", "root_binaries.txt"),
    AndroidExtractorStep::new("selinux_status", "selinux_status.txt"),
    AndroidExtractorStep::new("services", "services.txt"),
    AndroidExtractorStep::new("mounts", "mounts.txt"),
    AndroidExtractorStep::new("environment", "environment.txt"),
    AndroidExtractorStep::new("temp_files", "temp_files.txt"),
    AndroidExtractorStep::new("intrusion_indicators", "intrusion_indicators.txt"),
    AndroidExtractorStep::new("file_index", "file_index.txt"),
    AndroidExtractorStep::new("procfs_summary", "procfs_summary.txt"),
    AndroidExtractorStep::new("proc_memory_maps", "proc_memory_maps"),
    AndroidExtractorStep::new("heapdump_candidates", "heapdump_candidates.txt"),
    AndroidExtractorStep::new("debug_heap_dumps", "debug_heap_dumps"),
    AndroidExtractorStep::new("device_settings", "device_settings.txt"),
    AndroidExtractorStep::new("network_info", "network_info.txt"),
    AndroidExtractorStep::new("processes", "processes.txt"),
    AndroidExtractorStep::new("social_processes", "social_processes.json"),
    AndroidExtractorStep::new("disk_usage", "disk_usage.txt"),
    AndroidExtractorStep::new("content_sms", "content_sms.txt"),
    AndroidExtractorStep::new("content_calls", "content_calls.txt"),
    AndroidExtractorStep::new("content_contacts", "content_contacts.txt"),
    AndroidExtractorStep::new("content_user_dictionary", "content_user_dictionary.txt"),
    AndroidExtractorStep::new("content_calendar", "content_calendar.txt"),
    AndroidExtractorStep::new("content_media_images", "content_media_images.txt"),
    AndroidExtractorStep::new("content_media_videos", "content_media_videos.txt"),
    AndroidExtractorStep::new("content_media_audio", "content_media_audio.txt"),
    AndroidExtractorStep::new("content_media_files", "content_media_files.txt"),
    AndroidExtractorStep::new(
        "content_telephony_carriers",
        "content_telephony_carriers.txt",
    ),
    AndroidExtractorStep::new("screenshot", "screenshot.png"),
    AndroidExtractorStep::new("whatsapp_media", "whatsapp_media"),
    AndroidExtractorStep::new("telegram_media", "telegram_media"),
    AndroidExtractorStep::new("app_media", "app_media"),
    AndroidExtractorStep::new("all_app_media", "all_app_media"),
    AndroidExtractorStep::new("adb_backup", "adb_backup.ab"),
    AndroidExtractorStep::new("bugreport", "bugreport.zip"),
    AndroidExtractorStep::new("shared_storage", "shared_storage"),
];

/// Seçilen profile göre çalıştırılacak Android edinim adımlarını döndürür.
pub fn logical_steps_for_profile(profile: AndroidAcquisitionProfile) -> Vec<AndroidExtractorStep> {
    match profile {
        AndroidAcquisitionProfile::QuickLogical => filter_steps(QUICK_LOGICAL),
        AndroidAcquisitionProfile::FullLogical => FULL_LOGICAL_STEPS.to_vec(),
        AndroidAcquisitionProfile::RootLogical => {
            let mut steps = filter_steps(QUICK_LOGICAL);
            for step in FULL_LOGICAL_STEPS {
                if ROOT_LOGICAL_EXTRA.contains(&step.category)
                    && !steps
                        .iter()
                        .any(|current| current.category == step.category)
                {
                    steps.push(*step);
                }
            }
            steps
        }
        AndroidAcquisitionProfile::Volatile => filter_steps(VOLATILE_ONLY),
    }
}

/// Sabit kategori listesini tam adım tanımlarına indirger.
fn filter_steps(categories: &[&str]) -> Vec<AndroidExtractorStep> {
    categories
        .iter()
        .filter_map(|category| {
            FULL_LOGICAL_STEPS
                .iter()
                .find(|step| step.category == *category)
                .copied()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AndroidAcquisitionProfile, logical_steps_for_profile};

    #[test]
    fn quick_profile_is_smaller_than_full_profile() {
        let quick = logical_steps_for_profile(AndroidAcquisitionProfile::QuickLogical);
        let full = logical_steps_for_profile(AndroidAcquisitionProfile::FullLogical);
        assert!(quick.len() < full.len());
        assert!(quick.iter().any(|step| step.category == "content_sms"));
        assert!(!quick.iter().any(|step| step.category == "debug_heap_dumps"));
    }

    #[test]
    fn volatile_profile_keeps_memory_related_steps() {
        let steps = logical_steps_for_profile(AndroidAcquisitionProfile::Volatile);
        assert!(steps.iter().any(|step| step.category == "procfs_summary"));
        assert!(steps.iter().any(|step| step.category == "debug_heap_dumps"));
        assert!(!steps.iter().any(|step| step.category == "content_contacts"));
    }
}
