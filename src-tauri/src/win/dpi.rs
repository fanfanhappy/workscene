use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
};

/// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4（句柄式常量）
/// 必须最早调用，否则 GetWindowRect 返回 DWM 虚拟化坐标（评审 R2）
const PER_MONITOR_AWARE_V2: isize = -4;

pub fn enable_per_monitor_v2() -> bool {
    // 已由 manifest 设置时调用会失败，属正常情况
    unsafe {
        SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2 as *mut core::ffi::c_void) != 0
    }
}

pub fn window_dpi(hwnd: windows_sys::Win32::Foundation::HWND) -> u32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 {
            GetDpiForSystem()
        } else {
            dpi
        }
    }
}

pub fn scale_factor(dpi: u32) -> f64 {
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}
