use super::strings::from_wide;
use crate::core::models::ShowState;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowPlacement, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, ShowWindow,
    GWL_EXSTYLE, GW_OWNER, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL,
    WINDOWPLACEMENT, WS_EX_TOOLWINDOW,
};

type DwmGetWindowAttributeFn =
    unsafe extern "system" fn(HWND, u32, *mut core::ffi::c_void, u32) -> i32;

fn dwm_get_attr_fn() -> Option<DwmGetWindowAttributeFn> {
    static FN: OnceLock<Option<DwmGetWindowAttributeFn>> = OnceLock::new();
    *FN.get_or_init(|| unsafe {
        let dll: Vec<u16> = "dwmapi.dll\0".encode_utf16().collect();
        let module = LoadLibraryW(dll.as_ptr());
        if module.is_null() {
            return None;
        }
        let proc = GetProcAddress(module, b"DwmGetWindowAttribute\0".as_ptr());
        proc.map(|p| std::mem::transmute::<_, DwmGetWindowAttributeFn>(p))
    })
}

/// 动态加载 dwmapi!DwmGetWindowAttribute，检测跨虚拟桌面或挂起 UWP 的 Cloaked 幽灵窗口
fn is_cloaked(hwnd: HWND) -> bool {
    const DWMWA_CLOAKED: u32 = 14;
    let Some(dwm_get) = dwm_get_attr_fn() else {
        return false;
    };
    let mut cloaked: u32 = 0;
    let hr = unsafe {
        dwm_get(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    hr == 0 && cloaked != 0
}

/// 获取窗口的真实可见边框（去除 Win10/11 约 7px 透明阴影边框）
pub fn extended_frame_bounds(hwnd: HWND) -> Option<RECT> {
    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
    let dwm_get = dwm_get_attr_fn()?;
    let mut ext: RECT = unsafe { std::mem::zeroed() };
    let hr = unsafe {
        super::dpi::with_pmv2_context(|| {
            dwm_get(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut ext as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            )
        })
    };
    if hr == 0 && ext.right > ext.left && ext.bottom > ext.top {
        Some(ext)
    } else {
        None
    }
}

/// 计算 Win10/11 隐形阴影边框厚度 `(left, top, right, bottom)`，用于贴边分屏 0px 像素补偿
pub fn frame_inset(hwnd: HWND) -> (i32, i32, i32, i32) {
    if is_iconic(hwnd) || is_zoomed(hwnd) {
        return (0, 0, 0, 0);
    }
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut raw) } == 0 {
        return (0, 0, 0, 0);
    }
    let Some(ext) = extended_frame_bounds(hwnd) else {
        return (0, 0, 0, 0);
    };
    let left = (ext.left - raw.left).clamp(0, 24);
    let top = (ext.top - raw.top).clamp(0, 24);
    let right = (raw.right - ext.right).clamp(0, 24);
    let bottom = (raw.bottom - ext.bottom).clamp(0, 24);
    (left, top, right, bottom)
}

/// 把目标视觉矩形按隐形边框厚度外扩，让可见窗口边缘严格贴合目标位置
pub fn compensate_visual_rect(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let (l, t, r, b) = frame_inset(hwnd);
    (x - l, y - t, w + l + r, h + t + b)
}

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
/// 仅枚举可见顶层窗口（跳过约 95% 的隐藏后台窗口），大幅降低扫描耗时。
pub fn enumerate_raw() -> Vec<RawWindow> {
    enumerate_visible(None).0
}

struct EnumContext {
    out: Vec<RawWindow>,
    skip_pid: Option<u32>,
    skipped: usize,
}

/// 快速枚举可见顶层窗口，返回 `(可见窗口列表, 前置跳过数)`。
/// - 优先判断 `IsWindowVisible`：跳过海量隐藏系统/后台窗口，避免无谓的 `GetWindowTextW` / `GetWindowPlacement`。
/// - 若传入 `skip_pid`（正常捕获传自身 PID），直接跳过自身窗口，避免在工作线程对自身 WebView2 窗口触发同步 `WM_GETTEXT`。
/// - 对非最小化且零尺寸的窗口提前跳过。
pub fn enumerate_visible(skip_pid: Option<u32>) -> (Vec<RawWindow>, usize) {
    let mut ctx = EnumContext {
        out: Vec::with_capacity(48),
        skip_pid,
        skipped: 0,
    };
    let ptr = &mut ctx as *mut EnumContext as LPARAM;

    unsafe {
        EnumWindows(Some(enum_proc), ptr);
    }

    (ctx.out, ctx.skipped)
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut EnumContext);

    // 1. 绝大多数顶层窗口是不可见的系统/后台消息窗口，第一时间跳过
    if IsWindowVisible(hwnd) == 0 {
        ctx.skipped += 1;
        return TRUE;
    }

    // 1.5 过滤 DWM Cloaked 窗口（其他虚拟桌面的窗口、挂起 UWP/系统隐藏界面）
    if is_cloaked(hwnd) {
        ctx.skipped += 1;
        return TRUE;
    }

    // 2. 提前拿到 PID：正常捕获时直接跳过 WorkScene 自身窗口，避免对同进程 UI 线程发送 WM_GETTEXT
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if ctx.skip_pid == Some(pid) {
        ctx.skipped += 1;
        return TRUE;
    }

    // 3. 提前过滤非最小化的零尺寸窗口
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rect);
    let iconic = IsIconic(hwnd) != 0;
    if !iconic && (rect.right <= rect.left || rect.bottom <= rect.top) {
        ctx.skipped += 1;
        return TRUE;
    }

    let mut title_buf = [0u16; 512];
    GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
    let title = from_wide(&title_buf);

    let mut class_buf = [0u16; 256];
    GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
    let class_name = from_wide(&class_buf);

    let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
    wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    let placement_ok = GetWindowPlacement(hwnd, &mut wp) != 0;

    // 优先用 IsIconic / IsZoomed 判定当前真实状态（兼容 SW_MINIMIZE / SW_SHOWMINNOACTIVE 等变体）
    let show_state = if iconic {
        ShowState::Minimized
    } else if IsZoomed(hwnd) != 0 {
        ShowState::Maximized
    } else if placement_ok && wp.showCmd == SW_SHOWMAXIMIZED as u32 {
        ShowState::Maximized
    } else if placement_ok && wp.showCmd == SW_SHOWMINIMIZED as u32 {
        ShowState::Minimized
    } else {
        ShowState::Normal
    };

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let owner = GetWindow(hwnd, GW_OWNER);

    ctx.out.push(RawWindow {
        hwnd,
        pid,
        title,
        class_name,
        rect,
        restored_rect: if placement_ok { wp.rcNormalPosition } else { rect },
        show_state,
        is_visible: true,
        is_tool_window: ex_style & WS_EX_TOOLWINDOW != 0,
        has_owner: !owner.is_null(),
    });

    TRUE
}

/// 用于恢复：把位置写回窗口。始终不激活窗口（评审 U7），
/// 并在 PMv2 坐标语义下执行，避免目标进程 awareness 不同导致二次缩放。
pub fn set_window_rect(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_NOZORDER: u32 = 0x0004;
    let (cx, cy, cw, ch) = compensate_visual_rect(hwnd, x, y, w, h);
    unsafe {
        super::dpi::with_pmv2_context(|| {
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                cx,
                cy,
                cw,
                ch,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) != 0
        })
    }
}

/// 恢复显示状态（最大化 / 最小化 / 普通）与还原矩形（自动补偿 DWM 隐形边框）。
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

    let (cx, cy, cw, ch) = if state == ShowState::Normal {
        compensate_visual_rect(hwnd, x, y, w, h)
    } else {
        (x, y, w, h)
    };

    wp.showCmd = match state {
        ShowState::Maximized => SW_SHOWMAXIMIZED as u32,
        ShowState::Minimized => SW_SHOWMINNOACTIVE as u32,
        ShowState::Normal => SW_SHOWNORMAL as u32,
    };
    wp.rcNormalPosition = RECT {
        left: cx,
        top: cy,
        right: cx + cw,
        bottom: cy + ch,
    };

    unsafe {
        super::dpi::with_pmv2_context(|| {
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPlacement(hwnd, &wp) != 0
        })
    }
}

/// 最大化兜底：部分窗口（Chromium / Electron）不认 SetWindowPlacement 的 showCmd，直接调用 ShowWindow
pub fn show_maximized(hwnd: HWND) -> bool {
    unsafe { ShowWindow(hwnd, SW_SHOWMAXIMIZED) != 0 }
}

pub fn is_zoomed(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd) != 0 }
}

pub fn is_iconic(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) != 0 }
}

pub fn current_rect(hwnd: HWND) -> Option<RECT> {
    if !is_iconic(hwnd) && !is_zoomed(hwnd) {
        if let Some(ext) = extended_frame_bounds(hwnd) {
            return Some(ext);
        }
    }
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

/// 专注模式：把非本空间的窗口安静最小化到任务栏，绝不关闭或抢焦点
pub fn minimize_window(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWMINNOACTIVE};
    unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) != 0 }
}

/// 优雅关闭窗口：发送标准 WM_CLOSE（等价点击右上角 ×），有未保存文档时会正常提示，绝不强杀进程
pub fn close_window_gracefully(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 }
}
