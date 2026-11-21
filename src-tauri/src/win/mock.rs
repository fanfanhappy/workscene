//! 模拟应用窗口：给自动化测试一个完全可控的经典 Win32 顶层窗口。
//!
//! 为什么需要它：
//! - 不能依赖 notepad.exe：Windows 11 的记事本已是打包应用（ApplicationFrameWindow），
//!   按 v1.0 范围被过滤；
//! - 不能依赖 charmap.exe：从被 Job 约束的进程里 spawn GUI 程序会被立刻终止；
//! - 文档 §15 明确要求测试目录包含「模拟应用」，用于窗口识别与恢复的回归测试。
//!
//! 用法：`MockWindow::spawn(title, rect)` 创建并显示窗口，`close()` 关闭。

use std::sync::mpsc;
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW,
    PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG,
    SW_SHOW, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const CLASS_NAME: &str = "WorkSceneMockWindow";

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct MockWindow {
    pub hwnd: HWND,
    join: Option<JoinHandle<()>>,
}

impl MockWindow {
    /// 在独立线程里创建窗口并跑消息循环；返回时窗口已创建。
    pub fn spawn(title: &str, rect: (i32, i32, i32, i32)) -> Option<Self> {
        // HWND 是裸指针，不实现 Send，跨线程只传 usize
        let (tx, rx) = mpsc::channel::<Option<usize>>();
        let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let class_wide: Vec<u16> = CLASS_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        let join = std::thread::spawn(move || unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());

            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: (16 + 1) as *mut core::ffi::c_void, // COLOR_WINDOW + 1
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_wide.as_ptr(),
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                0,
                class_wide.as_ptr(),
                title_wide.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null_mut(),
            );

            if hwnd.is_null() {
                let _ = tx.send(None);
                return;
            }

            ShowWindow(hwnd, SW_SHOW);
            let _ = tx.send(Some(hwnd as usize));

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });

        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Some(raw)) => Some(MockWindow {
                hwnd: raw as HWND,
                join: Some(join),
            }),
            _ => {
                let _ = join.join();
                None
            }
        }
    }

    pub fn close(mut self) {
        unsafe {
            PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// 显式使用未引用的常量，保持与 Win32 语义一致的可读性
#[allow(dead_code)]
fn _ignored() -> (i32, i32) {
    (CW_USEDEFAULT, 0)
}
