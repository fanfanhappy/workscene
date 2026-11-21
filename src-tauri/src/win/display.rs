use super::dpi;
use super::strings::from_wide;
use windows_sys::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForMonitor;

const MDT_EFFECTIVE_DPI: i32 = 0;

#[derive(Clone)]
pub struct RawMonitor {
    pub key: String,
    pub device_name: String,
    pub rect: RECT,
    pub work: RECT,
    pub is_primary: bool,
    pub dpi: u32,
}

pub fn enumerate() -> Vec<RawMonitor> {
    let mut out: Vec<RawMonitor> = Vec::with_capacity(4);
    let ptr = &mut out as *mut Vec<RawMonitor> as LPARAM;
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_proc),
            ptr,
        );
    }
    out
}

unsafe extern "system" fn monitor_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> i32 {
    let out = &mut *(lparam as *mut Vec<RawMonitor>);

    let mut mi: MONITORINFOEXW = std::mem::zeroed();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmon, &mut mi.monitorInfo) != 0 {
        let is_primary = mi.monitorInfo.dwFlags & 1 == 1; // MONITORINFOF_PRIMARY
        let mrect = mi.monitorInfo.rcMonitor;
        let work = mi.monitorInfo.rcWork;

        let mut dpi_x: u32 = 96;
        let mut dpi_y: u32 = 96;
        let hr = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let dpi = if hr == 0 && dpi_x > 0 { dpi_x } else { 96 };

        // 优先用设备路径作为稳定标识；退化时用矩形，保证不为空
        let device = {
            let s = from_wide(&mi.szDevice);
            if s.trim().is_empty() {
                format!("MON-{}-{}", mrect.left, mrect.top)
            } else {
                s
            }
        };

        out.push(RawMonitor {
            key: device.clone(),
            device_name: device,
            rect: mrect,
            work,
            is_primary,
            dpi,
        });
    }

    TRUE
}

/// 窗口中心点落在哪个显示器；都不落在时取重叠面积最大者。
pub fn monitor_for_rect<'a>(rect: &RECT, monitors: &'a [RawMonitor]) -> Option<&'a RawMonitor> {
    if monitors.is_empty() {
        return None;
    }
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;

    if let Some(m) = monitors
        .iter()
        .find(|m| cx >= m.rect.left && cx < m.rect.right && cy >= m.rect.top && cy < m.rect.bottom)
    {
        return Some(m);
    }

    monitors
        .iter()
        .max_by_key(|m| overlap_area(rect, &m.rect))
        .or_else(|| monitors.first())
}

pub fn overlap_area(a: &RECT, b: &RECT) -> i64 {
    let l = a.left.max(b.left);
    let t = a.top.max(b.top);
    let r = a.right.min(b.right);
    let bo = a.bottom.min(b.bottom);
    if r <= l || bo <= t {
        return 0;
    }
    ((r - l) as i64) * ((bo - t) as i64)
}

pub fn scale_factor(dpi: u32) -> f64 {
    dpi::scale_factor(dpi)
}
