use serde::{Deserialize, Serialize};
use std::fs;
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

pub fn active_mounts() -> &'static Mutex<Vec<ActiveMount>> {
    static MOUNTS: OnceLock<Mutex<Vec<ActiveMount>>> = OnceLock::new();
    MOUNTS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn register_mount(mount: ActiveMount) {
    if let Ok(mut mounts) = active_mounts().lock() {
        mounts.push(mount);
    }
}

pub fn unregister_mount(mount_id: &str) {
    if let Ok(mut mounts) = active_mounts().lock() {
        mounts.retain(|m| m.mount_id != mount_id);
    }
}

pub fn list_active_mounts() -> Vec<ActiveMount> {
    if let Ok(mounts) = active_mounts().lock() {
        mounts.clone()
    } else {
        Vec::new()
    }
}

pub fn cleanup_all_mounts() -> Vec<String> {
    let mounts = list_active_mounts();
    let mut cleaned = Vec::new();
    for mount in mounts {
        if cleanup_single_mount(&mount) {
            unregister_mount(&mount.mount_id);
            cleaned.push(mount.mount_id.clone());
        }
    }
    cleaned
}

pub fn cleanup_case_mounts(case_name: &str) -> Vec<String> {
    let mounts = list_active_mounts();
    let mut cleaned = Vec::new();
    for mount in mounts {
        if mount.case_name == case_name {
            if cleanup_single_mount(&mount) {
                unregister_mount(&mount.mount_id);
                cleaned.push(mount.mount_id.clone());
            }
        }
    }
    cleaned
}

#[cfg(target_os = "linux")]
fn cleanup_single_mount(mount: &ActiveMount) -> bool {
    let _ = Command::new("umount").arg(&mount.mount_point).output();
    if let Some(loop_dev) = &mount.loop_device {
        let _ = Command::new("losetup").arg("-d").arg(loop_dev).output();
    }
    let _ = fs::remove_dir_all(&mount.mount_point);
    true
}

#[cfg(windows)]
fn cleanup_single_mount(mount: &ActiveMount) -> bool {
    let _ = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg("$ErrorActionPreference='Stop'; Dismount-DiskImage -ImagePath $args[0]")
        .arg(&mount.image_path)
        .output();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_unregister_mount() {
        let m = ActiveMount {
            mount_id: "test-id-123".to_string(),
            case_name: "test-case".to_string(),
            image_path: PathBuf::from("/tmp/img.dd"),
            mount_point: PathBuf::from("/mnt/test"),
            loop_device: None,
            mounted_at: "2024-01-01".to_string(),
        };
        register_mount(m);
        let list = list_active_mounts();
        assert!(list.iter().any(|x| x.mount_id == "test-id-123"));

        unregister_mount("test-id-123");
        let list2 = list_active_mounts();
        assert!(!list2.iter().any(|x| x.mount_id == "test-id-123"));
    }
}
