//! Edinim öncesi hedef diskte yeterli boş alan olup olmadığını denetler.
use crate::error::{AmeleError, AmeleResult, HataKodu};
use std::path::Path;

#[derive(Debug, serde::Serialize)]
pub struct PreflightResult {
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
        .map_err(|e| AmeleError::new(HataKodu::Genel, &e.to_string()))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    match unsafe { libc::statvfs(path_c.as_ptr(), &mut stat) } {
        0 => Ok(stat.f_bavail as u64 * stat.f_frsize as u64),
        _ => Err(AmeleError::new(HataKodu::Genel, "Boş alan okunamadı")),
    }
}

#[cfg(windows)]
pub fn check_available_space(target_path: &Path) -> AmeleResult<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut path_w: Vec<u16> = target_path.as_os_str().encode_wide().collect();
    path_w.push(0);
    let mut free: u64 = 0;
    match unsafe {
        GetDiskFreeSpaceExW(
            path_w.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } {
        0 => Err(AmeleError::new(HataKodu::Genel, "Boş alan okunamadı")),
        _ => Ok(free),
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
            // ioctl BLKGETSIZE64 blok aygıt boyutunu bayt cinsinden döndürür
            if unsafe { libc::ioctl(file.as_raw_fd(), 0x80081272, &mut size) } == 0 && size > 0 {
                return Ok(size);
            }
            Ok(file.metadata().map(|m| m.len()).unwrap_or(0))
        }
        "ram" => {
            let content = std::fs::read_to_string("/proc/meminfo")
                .map_err(|e| AmeleError::new(HataKodu::DosyaOkuma, &e.to_string()))?;
            let kb = content
                .lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            Ok(kb * 1024)
        }
        _ => Ok(0),
    }
}

#[cfg(windows)]
pub fn estimate_source_size(source_path: &str, source_type: &str) -> AmeleResult<u64> {
    match source_type {
        "disk" => Ok(std::fs::metadata(source_path).map(|m| m.len()).unwrap_or(0)),
        "ram" => {
            use windows_sys::Win32::System::SystemInformation::{
                GlobalMemoryStatusEx, MEMORYSTATUSEX,
            };
            let mut ms: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if unsafe { GlobalMemoryStatusEx(&mut ms) } != 0 {
                Ok(ms.ullTotalPhys)
            } else {
                Ok(0)
            }
        }
        _ => Ok(0),
    }
}

pub fn preflight_check(
    source_path: &str,
    source_type: &str,
    target_path: &Path,
) -> PreflightResult {
    let available = check_available_space(target_path).unwrap_or(0);
    let source_bytes = estimate_source_size(source_path, source_type).unwrap_or(0);
    let is_sufficient = source_bytes == 0 || available >= source_bytes;
    let shortage_bytes = if is_sufficient {
        0
    } else {
        source_bytes.saturating_sub(available)
    };
    let warning_message = (!is_sufficient).then(|| {
        format!(
            "Hedef diskte {:.1} GB boş alan var, kaynak {:.1} GB. Yetersiz.",
            available as f64 / 1_073_741_824.0,
            source_bytes as f64 / 1_073_741_824.0,
        )
    });
    PreflightResult {
        source_bytes,
        available_bytes: available,
        is_sufficient,
        shortage_bytes,
        warning_message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_available_space_is_nonzero() {
        assert!(check_available_space(Path::new("/tmp")).unwrap() > 0);
    }

    #[test]
    fn test_preflight_unknown_source() {
        let res = preflight_check("irrelevant", "android", Path::new("."));
        assert_eq!(res.source_bytes, 0);
        assert!(res.is_sufficient);
        assert!(res.warning_message.is_none());
    }
}
