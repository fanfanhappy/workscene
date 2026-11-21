use super::strings::from_wide;
use crate::core::models::ShowState;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowPlacement, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, GWL_EXSTYLE,
    GW_OWNER, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, SW_SHOWNORMAL, WINDOWPLACEMENT, WS_EX_TOOLWINDOW,
};

/// 原始窗口信息：只含系统直接给出的字段，不做任何业务判断。
/// 过滤与识别在 filter / matcher 中单独进行，便于单元测试。
#[derive(Clone)]
pub struct RawWindow {
    pub hwnd: HWND,
    pub pid: u32,
    pub title: String,
    pub class_name: String,
    pub rect: RECT,
    /// WINDOWPLACEMENT.rcNormalPosition：最小化时只有它是有效坐标
    pub restored_rect: RECT,
    pub show_state: ShowState,
    pub is_visible: bool,
    pub is_tool_window: bool,
    pub has_owner: bool,
}

/// 返回值顺序即 z-order（最上层在前），调用方据此写入 snapshot 的 zOrder。
pub fn enumerate_raw() -> Vec<RawWindow> {
    let mut out: Vec<RawWindow> = Vec::with_capacity(64);
    let ptr = &mut out as *mut Vec<RawWindow> as LPARAM;

    unsafe {
        EnumWindows(Some(enum_proc), ptr);
    }

    out
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let out = &mut *(lparam as *mut Vec<RawWindow>);

    let mut title_buf = [0u16; 512];
    GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
    let title = from_wide(&title_buf);

    let mut class_buf = [0u16; 256];
    GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
    let class_name = from_wide(&class_buf);

    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);

    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rect);

    let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
    wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    let placement_ok = GetWindowPlacement(hwnd, &mut wp) != 0;

    let show_state = if !placement_ok {
        ShowState::Normal
    } else {
        let cmd = wp.showCmd;
        if cmd == SW_SHOWMAXIMIZED as u32 {
            ShowState::Maximized
        } else if cmd == SW_SHOWMINIMIZED as u32 {
            ShowState::Minimized
        } else if cmd == SW_SHOWNORMAL as u32 {
            ShowState::Normal
        } else if IsZoomed(hwnd) != 0 {
            ShowState::Maximized
        } else if IsIconic(hwnd) != 0 {
            ShowState::Minimized
        } else {
            ShowState::Normal
        }
    };

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let owner = GetWindow(hwnd, GW_OWNER);

    out.push(RawWindow {
        hwnd,
        pid,
        title,
        class_name,
        rect,
        restored_rect: if placement_ok { wp.rcNormalPosition } else { rect },
        show_state,
        is_visible: IsWindowVisible(hwnd) != 0,
        is_tool_window: ex_style & WS_EX_TOOLWINDOW != 0,
        has_owner: !owner.is_null(),
    });

    TRUE
}

/// 用于恢复：把位置写回窗口。始终不激活窗口（评审 U7）。
pub fn set_window_rect(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_ASYNCWINDOWPOS: u32 = 0x4000;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_ASYNCWINDOWPOS,
        ) != 0
    }
}

/// 恢复显示状态（最大化）与还原矩形。
///
/// 关键顺序：先读当前 WINDOWPLACEMENT 以保留系统维护的字段，**再**覆盖
/// showCmd 与 rcNormalPosition，最后写回。反过来的话 GetWindowPlacement
/// 会把 showCmd 覆盖成窗口当前状态，最大化永远不会生效（评审补充意见 D1）。
pub fn apply_placement(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, state: ShowState) -> bool {
    let mut wp: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

    unsafe {
        GetWindowPlacement(hwnd, &mut wp);
    }

    wp.showCmd = match state {
        ShowState::Maximized => SW_SHOWMAXIMIZED as u32,
        ShowState::Minimized | ShowState::Normal => SW_SHOWNORMAL as u32,
    };
    wp.rcNormalPosition = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };

    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPlacement(hwnd, &wp) != 0 }
}

pub fn is_zoomed(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd) != 0 }
}

pub fn is_iconic(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) != 0 }
}

pub fn current_rect(hwnd: HWND) -> Option<RECT> {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut r) } != 0 {
        Some(r)
    } else {
        None
    }
}

pub fn is_window(hwnd: HWND) -> bool {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd) != 0 }
}
