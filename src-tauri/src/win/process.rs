use super::strings::from_wide;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    pub path: Option<String>,
    pub name: String,
    pub started_at: Option<i64>,
}

/// 只用 PROCESS_QUERY_LIMITED_INFORMATION：不请求提升权限，
/// 对系统进程/受保护进程失败时降级为 None（文档 §18 不默认提权）。
pub fn info(pid: u32) -> ProcessInfo {
    if pid == 0 {
        return ProcessInfo::default();
    }

    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return ProcessInfo::default();
        }

        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);

        let path = if ok != 0 && len > 0 {
            let full = from_wide(&buf[..len as usize]);
            Some(full)
        } else {
            None
        };

        let name = path
            .as_ref()
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let mut creation: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let started_at = if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0
        {
            Some(filetime_to_unix_ms(&creation))
        } else {
            None
        };

        CloseHandle(handle);

        ProcessInfo {
            path,
            name,
            started_at,
        }
    }
}

fn filetime_to_unix_ms(ft: &FILETIME) -> i64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    if ticks == 0 {
        return 0;
    }
    // 100ns since 1601-01-01 → ms since 1970-01-01
    (ticks / 10_000) as i64 - 11_644_473_600_000
}
