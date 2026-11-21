//! 系统托盘与全局快捷键（M7）。
//!
//! 设计取舍：
//! - 关闭主窗口 = 隐藏到托盘（工作空间管理器属于"常驻工具"，但退出必须明确）；
//! - 托盘菜单是唯一的退出入口，避免用户找不到怎么退出；
//! - 全局快捷键用 Win32 RegisterHotKey，而不是引第三方插件：只有一个组合键，
//!   自带消息循环更可控，也少一个依赖。

use crate::data;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

/// 全局快捷键：Ctrl+Alt+W 唤起主窗口
const HOTKEY_ID: i32 = 0x5753; // 'WS'

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let snap = MenuItem::with_id(app, "snapshot", "立即快照", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 WorkScene", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &snap, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("WorkScene — 工作现场管理器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "snapshot" => {
                let handle = app.clone();
                std::thread::spawn(move || {
                    take_snapshot_now(&handle);
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// 托盘菜单「立即快照」：给当前激活工作空间拍一份手动快照
fn take_snapshot_now(app: &AppHandle) {
    let db = app.state::<data::Db>().inner().clone();
    let capture = match crate::capture::scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("托盘快照失败：{e}");
            return;
        }
    };

    let _ = tauri::async_runtime::block_on(async {
        let Some(ws) = data::snapshots::active_workspace(&db).await? else {
            return Ok::<(), crate::core::AppError>(());
        };
        data::snapshots::create_manual(&db, &ws, "托盘快照", &capture).await?;
        Ok(())
    });
}

/// 窗口关闭 = 隐藏到托盘（退出走托盘菜单）
pub fn intercept_close(app: &tauri::App) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.handle().clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.hide();
            }
        }
    });
}

/// 在独立线程里注册全局快捷键并跑消息循环
pub fn spawn_hotkey(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        let registered = RegisterHotKey(
            std::ptr::null_mut(),
            HOTKEY_ID,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            'W' as u32,
        );
        if registered == 0 {
            eprintln!("全局快捷键 Ctrl+Alt+W 注册失败（可能被其它程序占用）");
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                show_main(&app);
            }
        }
    });
}
