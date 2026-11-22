//! 单实例：两个 WorkScene 同时跑会互相抢窗口、互相覆盖快照。

use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, TRUE};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, SetForegroundWindow, ShowWindow,
    SW_RESTORE, SW_SHOW,
};

/// 进程级互斥体句柄。故意不关闭：进程退出时由系统释放。
static MUTEX: OnceLock<usize> = OnceLock::new();

/// true 表示本进程是第一个实例。
pub fn acquire() -> bool {
    let name: Vec<u16> = "Local\\WorkScene.SingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            // 拿不到互斥体时不阻止启动，避免把权限问题变成"打不开应用"
            return true;
        }
        let _ = MUTEX.set(handle as usize);
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

struct FindExistingCtx {
    self_pid: u32,
    found: HWND,
}

unsafe extern "system" fn find_workscene_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut FindExistingCtx);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 || pid == ctx.self_pid {
        return TRUE;
    }

    let mut title_buf = [0u16; 128];
    let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
    if len <= 0 {
        return TRUE;
    }
    let title = String::from_utf16_lossy(&title_buf[..len as usize]);
    if title != "WorkScene" {
        return TRUE;
    }

    // 校验进程名必须是 workscene.exe，避免误激活名为 "WorkScene" 的文件夹或浏览器标签
    let pinfo = super::process::info(pid);
    if pinfo.name.eq_ignore_ascii_case("workscene.exe") {
        ctx.found = hwnd;
        return 0;
    }
    TRUE
}

/// 第二个实例启动时，把已存在的 WorkScene 主窗口拉到前台
pub fn focus_existing() {
    let mut ctx = FindExistingCtx {
        self_pid: std::process::id(),
        found: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(
            Some(find_workscene_proc),
            &mut ctx as *mut FindExistingCtx as LPARAM,
        );
        if !ctx.found.is_null() {
            // 主窗口关闭后处于 SW_HIDE 状态，先 SW_SHOW 再 SW_RESTORE 才能稳定唤回前台
            ShowWindow(ctx.found, SW_SHOW);
            ShowWindow(ctx.found, SW_RESTORE);
            SetForegroundWindow(ctx.found);
        }
    }
}
