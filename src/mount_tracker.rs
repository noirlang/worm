//! Aktif imaj mount'larını takip eder; vaka değişimi veya kapanışta temizler.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveMount {
    pub mount_id: String,
    pub case_name: String,
    pub image_path: PathBuf,
    pub mount_point: PathBuf,
    pub loop_device: Option<String>,
    pub mounted_at: String,
}

fn active_mounts() -> &'static Mutex<Vec<ActiveMount>> {
    static MOUNTS: OnceLock<Mutex<Vec<ActiveMount>>> = OnceLock::new();
    MOUNTS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn register_mount(mount: ActiveMount) {
    if let Ok(mut g) = active_mounts().lock() {
        g.push(mount);
    }
}

pub fn unregister_mount(mount_id: &str) {
    if let Ok(mut g) = active_mounts().lock() {
        g.retain(|m| m.mount_id != mount_id);
    }
}

pub fn list_active_mounts() -> Vec<ActiveMount> {
    active_mounts()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

fn do_cleanup(mounts: impl Iterator<Item = ActiveMount>) -> Vec<String> {
    mounts
        .filter(|m| cleanup_single_mount(m))
        .inspect(|m| unregister_mount(&m.mount_id))
        .map(|m| m.mount_id)
        .collect()
}

pub fn cleanup_all_mounts() -> Vec<String> {
    do_cleanup(list_active_mounts().into_iter())
}

pub fn cleanup_case_mounts(case_name: &str) -> Vec<String> {
    do_cleanup(
        list_active_mounts()
            .into_iter()
            .filter(|m| m.case_name == case_name),
    )
}

#[cfg(target_os = "linux")]
fn cleanup_single_mount(mount: &ActiveMount) -> bool {
    let _ = Command::new("umount").arg(&mount.mount_point).output();
    if let Some(dev) = &mount.loop_device {
        let _ = Command::new("losetup").arg("-d").arg(dev).output();
    }
    let _ = std::fs::remove_dir_all(&mount.mount_point);
    true
}

#[cfg(windows)]
fn cleanup_single_mount(mount: &ActiveMount) -> bool {
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "$ErrorActionPreference='Stop'; Dismount-DiskImage -ImagePath $args[0]",
        ])
        .arg(&mount.image_path)
        .output();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_unregister_mount() {
        let id = format!(
            "test-id-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        );
        register_mount(ActiveMount {
            mount_id: id.clone(),
            case_name: "test-case".into(),
            image_path: PathBuf::from("/tmp/img.dd"),
            mount_point: PathBuf::from("/mnt/test"),
            loop_device: None,
            mounted_at: "2024-01-01".into(),
        });
        assert!(list_active_mounts().iter().any(|m| m.mount_id == id));
        unregister_mount(&id);
        assert!(!list_active_mounts().iter().any(|m| m.mount_id == id));
    }
}
