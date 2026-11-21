//! 系统托盘与全局快捷键（M7 + 动态工作空间一键恢复）。
//!
//! 设计取舍：
//! - 托盘菜单直接列出前 6 个常用/收藏工作空间，一键切现场无需先打开主窗口；
//! - 工作空间增删改后调用 `refresh_menu` 实时更新托盘菜单；
//! - 关闭主窗口 = 隐藏到托盘，退出走托盘菜单。

use crate::data;
use tauri::menu::{Menu, MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "main-tray";
const RESTORE_PREFIX: &str = "restore:";

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn build_menu_with_list(
    app: &AppHandle,
    list: &[data::workspaces::WorkspaceSummary],
) -> tauri::Result<Menu<tauri::Wry>> {
    let mut builder = MenuBuilder::new(app);
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    builder = builder.item(&show);

    // 列出前 6 个工作空间（收藏优先）放入托盘快捷恢复区
    if !list.is_empty() {
        builder = builder.separator();
        for ws in list.iter().take(6) {
            let prefix = if ws.is_favorite { "★ " } else { "" };
            let label = format!("恢复：{prefix}{}", ws.name);
            let id = format!("{RESTORE_PREFIX}{}", ws.id);
            let item = MenuItem::with_id(app, id, label, true, None::<&str>)?;
            builder = builder.item(&item);
        }
    }

    let snap = MenuItem::with_id(app, "snapshot", "立即备份当前桌面", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 WorkScene", true, None::<&str>)?;
    builder.separator().item(&snap).separator().item(&quit).build()
}

/// 异步刷新托盘菜单：绝不在 Tokio 异步线程内调用 `block_on`，
/// 且把 Win32 菜单重建投递到主线程执行，避免跨线程风险或阻塞 IPC 返回。
pub async fn refresh_menu(app: &AppHandle) {
    let list = match app.try_state::<data::Db>() {
        Some(db) => data::workspaces::list(db.inner()).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            if let Ok(menu) = build_menu_with_list(&handle, &list) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    });
}

pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let list = app
        .try_state::<data::Db>()
        .and_then(|db| tauri::async_runtime::block_on(data::workspaces::list(db.inner())).ok())
        .unwrap_or_default();
    let menu = build_menu_with_list(app.handle(), &list)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("WorkScene — 工作现场管理器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if id == "show" {
                show_main(app);
            } else if id == "snapshot" {
                let handle = app.clone();
                std::thread::spawn(move || {
                    take_snapshot_now(&handle);
                });
            } else if id == "quit" {
                app.exit(0);
            } else if let Some(ws_id) = id.strip_prefix(RESTORE_PREFIX) {
                let handle = app.clone();
                let ws_id = ws_id.to_string();
                std::thread::spawn(move || {
                    restore_from_tray(&handle, &ws_id);
                });
            }
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

/// 托盘一键恢复：按全局设置自动启动缺失程序，恢复前自动备份，结果落库并通知前端
fn restore_from_tray(app: &AppHandle, workspace_id: &str) {
    let Some(_guard) = crate::restore::try_acquire() else {
        return;
    };
    let db = app.state::<data::Db>().inner().clone();

    let res = tauri::async_runtime::block_on(async {
        let scene = data::workspaces::to_scene(&db, workspace_id).await?;
        let snapshot_id = data::workspaces::latest_snapshot(&db, workspace_id).await?;
        let relaunch = data::settings::get(&db)
            .await
            .map(|s| s.restore_relaunch_default)
            .unwrap_or(true);

        let pre_restore = match crate::capture::scan() {
            Ok(current) => data::workspaces::add_snapshot(
                &db,
                workspace_id,
                "恢复前自动保护",
                "protection",
                "tray_restore",
                &current,
                true,
            )
            .await
            .ok(),
            Err(_) => None,
        };

        let report = crate::restore::apply(Some(app), &scene, relaunch, pre_restore)?;
        let _ = data::runs::save(
            &db,
            &report,
            Some(workspace_id),
            snapshot_id.as_deref(),
            "tray",
        )
        .await;
        let _ = data::workspaces::mark_restored(&db, workspace_id).await;
        let _ = data::snapshots::set_active_workspace(&db, workspace_id).await;
        Ok::<_, crate::core::AppError>(report)
    });

    if let Ok(report) = res {
        let _ = app.emit("tray:restore_done", &report);
    }
}

/// 托盘菜单「立即备份当前桌面」：给当前激活工作空间（若无则取首个工作空间）拍一份手动快照并通知前端
fn take_snapshot_now(app: &AppHandle) {
    let db = app.state::<data::Db>().inner().clone();
    let capture = match crate::capture::scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("托盘快照失败：{e}");
            return;
        }
    };

    let created = tauri::async_runtime::block_on(async {
        let ws = match data::snapshots::active_workspace(&db).await? {
            Some(id) => id,
            None => {
                let list = data::workspaces::list(&db).await?;
                let Some(first) = list.first() else {
                    return Ok::<Option<data::snapshots::SnapshotSummary>, crate::core::AppError>(
                        None,
                    );
                };
                first.id.clone()
            }
        };
        let snap = data::snapshots::create_manual(&db, &ws, "托盘备份", &capture).await?;
        Ok(Some(snap))
    });

    if let Ok(Some(snap)) = created {
        let _ = app.emit("tray:snapshot_done", &snap);
    }
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

/// 在独立线程里注册全局快捷键并跑消息循环（实现已移到 win::hotkey）
pub fn spawn_hotkey(app: AppHandle, preset: &str) {
    crate::win::hotkey::spawn(app, crate::win::hotkey::parse_preset(preset));
}
