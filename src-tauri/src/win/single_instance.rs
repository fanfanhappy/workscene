//! 单实例：两个 WorkScene 同时跑会互相抢窗口、互相覆盖快照。

use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
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

/// 第二个实例启动时，把已存在的窗口拉到前台
pub fn focus_existing() {
    let title: Vec<u16> = "WorkScene"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            // 主窗口关闭后处于 SW_HIDE 状态，先 SW_SHOW 再 SW_RESTORE 才能稳定唤回前台
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}
