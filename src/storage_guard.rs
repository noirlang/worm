use crate::error::{AmeleError, AmeleResult, HataKodu};
use crate::logging::{LogLevel, runtime_log};
use std::path::Path;

#[derive(Debug, serde::Serialize)]
pub struct PreflightResult {
    pub ok: bool,
    pub source_bytes: u64,
    pub available_bytes: u64,
    pub is_sufficient: bool,
    pub shortage_bytes: u64,
    pub warning_message: Option<String>,
}

#[cfg(target_os = "linux")]
pub fn check_available_space(target_path: &Path) -> AmeleResult<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_c = CString::new(target_path.as_os_str().as_bytes())
        .map_err(|e| AmeleError::new(HataKodu::Genel, &format!("Geçersiz yol: {}", e)))?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::statvfs(path_c.as_ptr(), &mut stat) };
    if res == 0 {
        Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
    } else {
        Err(AmeleError::new(HataKodu::Genel, "Boş alan okunamadı"))
    }
}

#[cfg(windows)]
pub fn check_available_space(target_path: &Path) -> AmeleResult<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut path_w: Vec<u16> = target_path.as_os_str().encode_wide().collect();
    path_w.push(0);

    let mut free_bytes_available: u64 = 0;
    let mut total_number_of_bytes: u64 = 0;
    let mut total_number_of_free_bytes: u64 = 0;

    let res = unsafe {
        GetDiskFreeSpaceExW(
            path_w.as_ptr(),
            &mut free_bytes_available,
            &mut total_number_of_bytes,
            &mut total_number_of_free_bytes,
        )
    };

    if res != 0 {
        Ok(free_bytes_available)
    } else {
        Err(AmeleError::new(HataKodu::Genel, "Boş alan okunamadı"))
    }
}

#[cfg(target_os = "linux")]
pub fn estimate_source_size(source_path: &str, source_type: &str) -> AmeleResult<u64> {
    match source_type {
        "disk" => {
            use std::fs::File;
            use std::os::unix::io::AsRawFd;

            let file = File::open(source_path)
                .map_err(|e| AmeleError::new(HataKodu::DosyaOkuma, &e.to_string()))?;
            let mut size: u64 = 0;
            // BLKGETSIZE64 is 0x80081272
            const BLKGETSIZE64: u64 = 0x80081272;
            let res = unsafe { libc::ioctl(file.as_raw_fd(), BLKGETSIZE64, &mut size) };
            if res == 0 {
                Ok(size)
            } else {
                if let Ok(metadata) = file.metadata() {
                    Ok(metadata.len())
                } else {
                    Err(AmeleError::new(HataKodu::Genel, "Boyut alınamadı"))
                }
            }
        }
        "ram" => {
            let content = std::fs::read_to_string("/proc/meminfo")
                .map_err(|e| AmeleError::new(HataKodu::DosyaOkuma, &e.to_string()))?;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return Ok(kb * 1024);
                        }
                    }
                }
            }
            Ok(0)
        }
        "android" | "ios" => Ok(0),
        _ => Ok(0),
    }
}

#[cfg(windows)]
pub fn estimate_source_size(source_path: &str, source_type: &str) -> AmeleResult<u64> {
    match source_type {
        "disk" => {
            if let Ok(metadata) = std::fs::metadata(source_path) {
                Ok(metadata.len())
            } else {
                Err(AmeleError::new(HataKodu::Genel, "Boyut alınamadı"))
            }
        }
        "ram" => {
            use windows_sys::Win32::System::SystemInformation::{
                GlobalMemoryStatusEx, MEMORYSTATUSEX,
            };
            let mut mem_status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
            mem_status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            let res = unsafe { GlobalMemoryStatusEx(&mut mem_status) };
            if res != 0 {
                Ok(mem_status.ullTotalPhys)
            } else {
                Ok(0)
            }
        }
        "android" | "ios" => Ok(0),
        _ => Ok(0),
    }
}

pub fn preflight_check(
    source_path: &str,
    source_type: &str,
    target_path: &Path,
) -> PreflightResult {
    runtime_log(
        LogLevel::Info,
        "storage_guard",
        format!(
            "Preflight kontrolü: {} -> {}",
            source_path,
            target_path.display()
        ),
    );

    let available = check_available_space(target_path).unwrap_or(0);
    let source_size = estimate_source_size(source_path, source_type).unwrap_or(0);

    let is_sufficient = if source_size == 0 {
        true
    } else {
        available >= source_size
    };

    let shortage = if is_sufficient {
        0
    } else {
        source_size.saturating_sub(available)
    };

    let warning = if !is_sufficient {
        Some(format!(
            "Yetersiz disk alanı. Tahmini gereken: {} byte, Mevcut: {} byte",
            source_size, available
        ))
    } else {
        None
    };

    PreflightResult {
        ok: true,
        source_bytes: source_size,
        available_bytes: available,
        is_sufficient,
        shortage_bytes: shortage,
        warning_message: warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_available_space_is_nonzero() {
        let available = check_available_space(Path::new("/tmp")).unwrap();
        assert!(available > 0);
    }

    #[test]
    fn test_preflight_unknown_source() {
        let res = preflight_check("some_device", "android", Path::new("."));
        assert!(res.ok);
        assert_eq!(res.source_bytes, 0);
        assert!(res.is_sufficient);
    }
}
