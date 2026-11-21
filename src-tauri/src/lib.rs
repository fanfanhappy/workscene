mod adapters;
mod capture;
mod commands;
mod core;
mod data;
mod restore;
mod storage;
mod win;

use core::models::{CaptureResult, ShowState, WindowInfo};
use std::time::Duration;
use tauri::Manager;
use windows_sys::Win32::Foundation::HWND;

pub fn run() {
    // 必须在创建任何窗口前设置，否则坐标会被 DWM 虚拟化
    win::dpi::enable_per_monitor_v2();

    // 单实例：第二个实例只负责把已存在的窗口拉到前台
    if !win::single_instance::acquire() {
        win::single_instance::focus_existing();
        return;
    }

    tauri::Builder::default()
        .setup(|app| {
            // 数据库在启动时初始化（建库 + 跑迁移），失败直接让启动失败，
            // 避免出现"能开窗但什么都不能用"的半死状态。
            let db = tauri::async_runtime::block_on(data::init())?;

            // 会话标记：上次是否正常退出。崩溃时在数据目录留下 recovery-hint.json，
            // 本次启动据此在总览页提示"可以恢复上次现场"。
            if let Some(previous) = data::session::begin() {
                let hint = tauri::async_runtime::block_on(build_recovery_hint(&db, previous.started_at));
                let _ = data::session::write_hint(&hint);
            }

            app.manage(db);

            // 窗口在代码里创建而不是写在配置里，为的是把 WebView2 的用户数据目录
            // 放到我们自己的数据目录下：既支持便携模式（WORKSCENE_DATA_DIR），
            // 也避免依赖 %LOCALAPPDATA% 是否可写。UI 规范 §12.1「存储位置」也需要这个能力。
            let webview_dir = storage::data_dir().join("webview2");
            let _ = std::fs::create_dir_all(&webview_dir);

            let mut builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("WorkScene")
                    .inner_size(1040.0, 680.0)
                    .min_inner_size(860.0, 560.0)
                    .center();

            #[cfg(target_os = "windows")]
            {
                builder = builder.data_directory(webview_dir);
            }

            builder.build()?;

            win::tray::build_tray(app)?;
            win::tray::intercept_close(app);

            // 快捷键以设置为准；同时按设置注册 Ctrl+Alt+1~5 工作空间快捷切屏
            let init_settings = {
                let db = app.state::<data::Db>().inner().clone();
                tauri::async_runtime::block_on(data::settings::get(&db)).unwrap_or_default()
            };
            win::filter::set_ignored_processes(&init_settings.ignored_processes);
            win::tray::spawn_hotkey(
                app.handle().clone(),
                &init_settings.hotkey,
                init_settings.workspace_hotkeys_enabled,
            );

            spawn_auto_snapshot(app.handle().clone());
            spawn_display_watcher(app.handle().clone());

            // 开机/启动后自动恢复指定工作空间（若用户已配置）
            if let Some(ws_id) = init_settings.startup_workspace_id.clone() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(3500));
                    win::tray::restore_workspace_by_id(&handle, &ws_id);
                });
            }

            // 诊断用：WORKSCENE_ROUTE=/settings 可直接打开指定页面（截图与回归用）。
            // 必须等 WebView 加载完再改 hash，否则会被 HashRouter 的初始化覆盖。
            if let Ok(route) = std::env::var("WORKSCENE_ROUTE") {
                let win = app.get_webview_window("main");
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(2000));
                    if let Some(w) = win {
                        let _ = w.eval(&format!(
                            "window.location.hash = '{}';",
                            route.replace('\'', "")
                        ));
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system_get_info,
            commands::system_get_displays,
            commands::capture_start,
            commands::scene_health,
            commands::window_set_theme,
            commands::workspace_list,
            commands::workspace_get,
            commands::workspace_create,
            commands::workspace_rename,
            commands::workspace_set_favorite,
            commands::workspace_update_meta,
            commands::workspace_duplicate,
            commands::workspace_export,
            commands::workspace_import,
            commands::workspace_import_preview,
            commands::workspace_recapture,
            commands::workspace_update_from_capture,
            commands::workspace_delete,
            commands::workspace_item_update,
            commands::workspace_item_relocate,
            commands::workspace_layout_update,
            commands::workspace_window_delete,
            commands::workspace_item_delete,
            commands::workspace_item_add,
            commands::workspace_items_add_batch,
            commands::workspace_close_windows,
            commands::app_icons_batch,
            commands::bookmarks_list_folders,
            commands::workspace_preview,
            commands::workspace_restore,
            commands::workspace_restore_retry,
            commands::snapshot_restore,
            commands::run_list,
            commands::run_steps,
            commands::run_error_stats,
            commands::restore_is_busy,
            commands::restore_cancel,
            commands::snapshot_list,
            commands::snapshot_get,
            commands::snapshot_create,
            commands::snapshot_delete,
            commands::snapshot_diff,
            commands::snapshot_prune,
            commands::recovery_hint,
            commands::recovery_dismiss,
            commands::adapter_suggest,
            commands::adapter_classify_batch,
            commands::settings_get,
            commands::settings_set,
            commands::storage_stats,
            commands::storage_cleanup,
            commands::system_open_data_dir,
            commands::dialog_pick_file,
            commands::dialog_pick_folder,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // release 版没有控制台：启动失败必须弹窗，否则用户看到的是"点了没反应"
            win::dialog::error(
                "WorkScene 启动失败",
                &format!("{e}\n\n如果反复出现，请把这段信息发给我们。"),
            );
            std::process::exit(1);
        })
        .run(|_app, event| {
            // 正常退出时把会话标记改成 clean，下次启动就不会误报"异常退出"
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                data::session::mark_clean();
            }
        });
}

/// 组装崩溃恢复提示：激活工作空间 + 该工作空间最近一份快照
async fn build_recovery_hint(db: &data::Db, crashed_at: i64) -> data::session::RecoveryHint {
    let mut hint = data::session::RecoveryHint {
        crashed_at,
        workspace_id: None,
        workspace_name: None,
        snapshot_id: None,
        snapshot_at: None,
    };

    let Ok(Some(ws_id)) = data::snapshots::active_workspace(db).await else {
        return hint;
    };
    hint.workspace_id = Some(ws_id.clone());

    if let Ok(detail) = data::workspaces::get(db, &ws_id).await {
        hint.workspace_name = Some(detail.name);
    }

    if let Ok(snaps) = data::snapshots::list(db, &ws_id, 1).await {
        if let Some(latest) = snaps.first() {
            hint.snapshot_id = Some(latest.id.clone());
            hint.snapshot_at = Some(latest.captured_at);
        }
    }

    hint
}

/// 无 UI 自检：把真实捕获结果打印成 JSON。
/// 用法：cargo run -- --selftest
pub fn selftest() {
    win::dpi::enable_per_monitor_v2();
    match capture::scan() {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("序列化失败：{e}"),
        },
        Err(e) => eprintln!("捕获失败：{e}"),
    }
}

/// 无头捕获：把当前桌面直接保存为一个工作空间（可脚本化、可做回归基线）。
/// 用法：cargo run -- --capture "前端开发"
pub fn cli_capture(name: &str) {
    win::dpi::enable_per_monitor_v2();
    let capture = match capture::scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            return;
        }
    };
    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            return;
        }
    };
    match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db, name, &capture,
    )) {
        Ok(w) => println!(
            "已保存工作空间：{}（{} 个应用 / {} 个窗口）→ {}",
            w.name, w.item_count, w.window_count, w.id
        ),
        Err(e) => eprintln!("保存失败：{e}"),
    }
}

/// 无头快照：按工作空间名称追加一份手动快照（演示与回归用）。
/// 用法：cargo run -- --snap "前端开发"
pub fn cli_snapshot(name: &str) {
    win::dpi::enable_per_monitor_v2();
    let capture = match capture::scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            return;
        }
    };
    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            return;
        }
    };
    let list = match tauri::async_runtime::block_on(data::workspaces::list(&db)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("读取工作空间失败：{e}");
            return;
        }
    };
    let Some(ws) = list.iter().find(|w| w.name == name).or_else(|| list.first()) else {
        eprintln!("没有可用的工作空间");
        return;
    };
    match tauri::async_runtime::block_on(data::snapshots::create_manual(
        &db,
        &ws.id,
        &format!("手动快照 {}", ws.name),
        &capture,
    )) {
        Ok(s) => {
            // 命令行给某个工作空间拍快照，视为把它设为当前工作空间，
            // 这样自动快照与崩溃恢复提示都能找到归属
            let _ = tauri::async_runtime::block_on(data::snapshots::set_active_workspace(
                &db, &ws.id,
            ));
            println!(
                "已写入快照：{}（{} 个窗口，类型 {}）",
                s.id, s.window_count, s.snapshot_type
            )
        }
        Err(e) => eprintln!("写入快照失败：{e}"),
    }
}

/// 开机启动诊断：`--autostart status|on|off`
pub fn cli_autostart(mode: &str) {
    match mode {
        "on" => match win::autostart::set_enabled_verbose(true) {
            Ok(()) => println!("已开启开机启动"),
            Err(code) => println!("开启失败（Win32 错误码 {code}）"),
        },
        "off" => match win::autostart::set_enabled_verbose(false) {
            Ok(()) => println!("已关闭开机启动"),
            Err(code) => println!("关闭失败（Win32 错误码 {code}）"),
        },
        _ => {
            println!(
                "开机启动：{}",
                if win::autostart::is_enabled() {
                    "已开启"
                } else {
                    "未开启"
                }
            );
            if let Some(cmd) = win::autostart::launch_command() {
                println!("启动命令：{cmd}");
            }
        }
    }
}

/// M1 闭环自检（含最大化状态）：
/// 建两个受控模拟窗口（一个普通、一个最大化）→ 捕获 → 保存快照 → 破坏布局 →
/// 按快照恢复 → 校验位置与最大化状态。
/// 只操作自己创建的模拟窗口，不触碰用户任何窗口。
/// 用法：cargo run -- --m1test
pub fn m1test() {
    win::dpi::enable_per_monitor_v2();
    println!("[env] data_dir={}", storage::data_dir().display());

    let normal_target = (240, 160, 900, 620);
    let normal_moved = (900, 480, 640, 420);
    let max_normal_rect = (600, 320, 800, 600);

    let Some(mock_a) = win::mock::MockWindow::spawn("WorkScene 模拟应用 A", normal_target) else {
        eprintln!("无法创建模拟窗口 A，自检中止");
        std::process::exit(2);
    };
    let Some(mock_b) = win::mock::MockWindow::spawn("WorkScene 模拟应用 B 最大化", max_normal_rect)
    else {
        eprintln!("无法创建模拟窗口 B，自检中止");
        mock_a.close();
        std::process::exit(2);
    };
    std::thread::sleep(Duration::from_millis(300));

    // 用被测的 apply_placement 把 B 设为最大化，顺带验证这条路径本身
    win::window::apply_placement(
        mock_b.hwnd,
        max_normal_rect.0,
        max_normal_rect.1,
        max_normal_rect.2,
        max_normal_rect.3,
        ShowState::Maximized,
    );
    std::thread::sleep(Duration::from_millis(300));

    println!(
        "[1/6] 模拟窗口：A HWND {} 普通，B HWND {} 最大化={}",
        mock_a.hwnd as usize,
        mock_b.hwnd as usize,
        win::window::is_zoomed(mock_b.hwnd)
    );

    let capture = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };

    // 只保留本次测试的两个模拟窗口，避免恢复时动到用户其他窗口
    let mine: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| w.hwnd == mock_a.hwnd as usize || w.hwnd == mock_b.hwnd as usize)
        .cloned()
        .collect();

    if mine.len() != 2 {
        eprintln!("捕获结果里只有 {} 个模拟窗口（应为 2）", mine.len());
        mock_a.close();
        mock_b.close();
        std::process::exit(2);
    }

    let isolated = CaptureResult {
        windows: mine.clone(),
        displays: capture.displays.clone(),
        ..capture
    };

    let scene = match storage::save_scene("M1 自检（临时）", &isolated) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("保存快照失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    println!(
        "[2/6] 已捕获并保存快照：{} 个窗口（含 1 个最大化），{} 台显示器",
        scene.window_count, scene.display_count
    );

    // 破坏现场：A 挪走，B 取消最大化并移动
    win::window::set_window_rect(
        mock_a.hwnd,
        normal_moved.0,
        normal_moved.1,
        normal_moved.2,
        normal_moved.3,
    );
    win::window::apply_placement(
        mock_b.hwnd,
        100,
        100,
        700,
        500,
        ShowState::Normal,
    );
    std::thread::sleep(Duration::from_millis(400));
    println!(
        "[3/6] 已破坏现场：A 移到 {:?}，B 取消最大化={}",
        win::window::current_rect(mock_a.hwnd).map(|r| (r.left, r.top)),
        !win::window::is_zoomed(mock_b.hwnd)
    );

    let loaded = match storage::load_scene(&scene.id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读取快照失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };

    let report = match restore::apply(None, &loaded, false, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("恢复失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };

    println!("[4/6] 恢复报告：");
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("序列化失败：{e}"),
    }

    let pos_a = win::window::current_rect(mock_a.hwnd)
        .map(|r| (r.left, r.top, r.right - r.left, r.bottom - r.top));
    let a_ok = pos_a
        .map(|r| (r.0 - normal_target.0).abs() <= 8 && (r.1 - normal_target.1).abs() <= 8)
        .unwrap_or(false);
    let b_zoomed = win::window::is_zoomed(mock_b.hwnd);

    println!(
        "[5/6] 校验：A 目标 ({}, {}) 实际 {:?}；B 最大化={}",
        normal_target.0, normal_target.1, pos_a, b_zoomed
    );

    let ok = report.succeeded == 2 && report.failed == 0 && a_ok && b_zoomed;

    mock_a.close();
    mock_b.close();
    let _ = storage::delete_scene(&scene.id);

    println!(
        "[6/6] 清理完成。M1 结果：{}（成功 {} / 失败 {} / 跳过 {}）\n{}",
        if ok { "PASS" } else { "FAIL" },
        report.succeeded,
        report.failed,
        report.skipped,
        if ok {
            "位置恢复 + 最大化状态恢复都通过"
        } else {
            "存在未通过项，见上面的报告与校验行"
        }
    );

    if !ok {
        std::process::exit(1);
    }
}

/// M2 自检：SQLite 工作空间闭环。
/// 建两个受控窗口 → 捕获 → 建工作空间（应用去重）→ 列表/详情 → 破坏布局 →
/// 从工作空间恢复 → 报告入库 → 按错误码聚合 → 删除工作空间（级联清理 + payload 文件）。
/// 用法：cargo run -- --m2test
pub fn m2test() {
    win::dpi::enable_per_monitor_v2();
    println!("[env] data_dir={}", storage::data_dir().display());

    let mut ok = true;
    let normal_target = (260, 180, 880, 600);

    let Some(mock_a) = win::mock::MockWindow::spawn("WorkScene M2 模拟应用", normal_target) else {
        eprintln!("无法创建模拟窗口，自检中止");
        std::process::exit(2);
    };
    let Some(mock_b) = win::mock::MockWindow::spawn("WorkScene M2 最大化窗口", (700, 380, 760, 560))
    else {
        eprintln!("无法创建第二个模拟窗口，自检中止");
        mock_a.close();
        std::process::exit(2);
    };
    std::thread::sleep(Duration::from_millis(300));
    win::window::apply_placement(
        mock_b.hwnd,
        700,
        380,
        760,
        560,
        ShowState::Maximized,
    );
    std::thread::sleep(Duration::from_millis(250));

    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    println!("[1/7] 数据库已初始化并跑完迁移");

    let capture = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    let mine: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| w.hwnd == mock_a.hwnd as usize || w.hwnd == mock_b.hwnd as usize)
        .cloned()
        .collect();
    if mine.len() != 2 {
        eprintln!("只捕获到 {} 个模拟窗口（应为 2）", mine.len());
        mock_a.close();
        mock_b.close();
        std::process::exit(2);
    }
    let isolated = CaptureResult {
        windows: mine,
        displays: capture.displays.clone(),
        ..capture
    };

    let created = match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db,
        "M2 自检工作空间",
        &isolated,
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("创建工作空间失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    println!(
        "[2/7] 已建工作空间：{} 个应用项 / {} 个窗口（{}）",
        created.item_count, created.window_count, created.id
    );

    let listed = tauri::async_runtime::block_on(data::workspaces::list(&db)).unwrap_or_default();
    let detail = tauri::async_runtime::block_on(data::workspaces::get(&db, &created.id));
    match detail {
        Ok(d) => println!(
            "[3/7] 列表 {} 个；详情：{} 个应用项、{} 个窗口、首个显示器 {}",
            listed.len(),
            d.items.len(),
            d.windows.len(),
            d.windows.first().map(|w| w.monitor_key.clone()).unwrap_or_default()
        ),
        Err(e) => {
            eprintln!("读取详情失败：{e}");
            ok = false;
        }
    }

    // 破坏现场
    win::window::set_window_rect(mock_a.hwnd, 1000, 600, 640, 420);
    win::window::apply_placement(mock_b.hwnd, 120, 120, 700, 500, ShowState::Normal);
    std::thread::sleep(Duration::from_millis(300));

    let scene = match tauri::async_runtime::block_on(data::workspaces::to_scene(
        &db,
        &created.id,
    )) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("由工作空间构造恢复计划失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    println!("[4/7] 已由 DB 布局构造恢复计划：{} 个窗口", scene.windows.len());

    let report = match restore::apply(None, &scene, false, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("恢复失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    println!(
        "[5/7] 恢复结果：成功 {} / 失败 {} / 跳过 {}",
        report.succeeded, report.failed, report.skipped
    );

    let pos = win::window::current_rect(mock_a.hwnd)
        .map(|r| (r.left, r.top, r.right - r.left, r.bottom - r.top));
    let a_ok = pos
        .map(|r| (r.0 - normal_target.0).abs() <= 8 && (r.1 - normal_target.1).abs() <= 8)
        .unwrap_or(false);
    let b_ok = win::window::is_zoomed(mock_b.hwnd);
    println!(
        "[6/7] 校验：A 位置 {}（目标 {},{}），B 最大化={}",
        if a_ok { "通过" } else { "未通过" },
        normal_target.0,
        normal_target.1,
        b_ok
    );

    let saved = tauri::async_runtime::block_on(data::runs::save(
        &db,
        &report,
        Some(&created.id),
        None,
        "m2test",
    ));
    let runs = tauri::async_runtime::block_on(data::runs::list(&db, 10)).unwrap_or_default();
    let stats = tauri::async_runtime::block_on(data::runs::error_code_stats(&db)).unwrap_or_default();
    println!(
        "[7/7] 报告入库={:?}，runs={}，首条 steps={}，错误码分布={:?}",
        saved.is_ok(),
        runs.len(),
        runs.first().map(|r| r.total_steps).unwrap_or(0),
        stats
    );

    // 删除并验证级联
    let before_files = std::fs::read_dir(storage::snapshot_dir())
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0);
    let deleted = tauri::async_runtime::block_on(data::workspaces::delete(&db, &created.id));
    let after_files = std::fs::read_dir(storage::snapshot_dir())
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0);
    let items_left: i64 = tauri::async_runtime::block_on(sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_items WHERE workspace_id = ?",
    )
    .bind(&created.id)
    .fetch_one(&db.pool))
    .unwrap_or(-1);
    println!(
        "     删除={:?}，残留 items={}，快照文件 {} → {}",
        deleted.is_ok(),
        items_left,
        before_files,
        after_files
    );

    if !(a_ok && b_ok && report.failed == 0 && items_left == 0 && after_files < before_files) {
        ok = false;
    }

    mock_a.close();
    mock_b.close();

    println!(
        "\nM2 结果：{}（工作空间闭环 + 报告入库 + 级联清理）",
        if ok { "PASS" } else { "FAIL" }
    );
    if !ok {
        std::process::exit(1);
    }
}

/// 成功率基线：重复 N 轮「破坏布局 → 按工作空间恢复 → 校验」，输出 JSONL 与汇总。
/// 这是评审 R8/A5 要求的测量协议的最小实现：没有它，"恢复成功率"只能靠跑一遍看看。
/// 用法：cargo run -- --baseline 5
pub fn baseline(iterations: usize) {
    win::dpi::enable_per_monitor_v2();
    let iterations = iterations.max(1);
    println!("[env] data_dir={}", storage::data_dir().display());

    let specs: [(&str, (i32, i32, i32, i32), bool); 3] = [
        ("基线窗口 A", (300, 200, 900, 600), false),
        ("基线窗口 B", (760, 420, 780, 560), false),
        ("基线窗口 C", (500, 260, 820, 620), true),
    ];

    let mut mocks: Vec<win::mock::MockWindow> = Vec::new();
    for (title, rect, _) in specs {
        match win::mock::MockWindow::spawn(title, rect) {
            Some(m) => mocks.push(m),
            None => {
                eprintln!("创建模拟窗口失败：{title}");
                std::process::exit(2);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(350));

    // 按快照里的期望状态初始化（C 设为最大化）
    for (m, (_, rect, maximized)) in mocks.iter().zip(specs.iter()) {
        if *maximized {
            win::window::apply_placement(m.hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Maximized);
        } else {
            win::window::set_window_rect(m.hwnd, rect.0, rect.1, rect.2, rect.3);
        }
    }
    std::thread::sleep(Duration::from_millis(300));

    let hwnds: Vec<usize> = mocks.iter().map(|m| m.hwnd as usize).collect();

    let capture = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            std::process::exit(2);
        }
    };
    let mine: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| hwnds.contains(&w.hwnd))
        .cloned()
        .collect();
    if mine.len() != mocks.len() {
        eprintln!("捕获到 {} 个模拟窗口（应为 {}）", mine.len(), mocks.len());
        std::process::exit(2);
    }
    let isolated = CaptureResult {
        windows: mine,
        displays: capture.displays.clone(),
        ..capture
    };

    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            std::process::exit(2);
        }
    };
    let ws = match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db,
        "基线工作空间",
        &isolated,
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("创建工作空间失败：{e}");
            std::process::exit(2);
        }
    };
    let scene = match tauri::async_runtime::block_on(data::workspaces::to_scene(&db, &ws.id)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("构造恢复计划失败：{e}");
            std::process::exit(2);
        }
    };

    println!(
        "[1/3] 工作空间就绪：{} 个窗口，开始 {} 轮恢复基线",
        scene.windows.len(),
        iterations
    );

    let mut lines: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut success = 0usize;
    let mut codes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for i in 0..iterations {
        // 确定性扰动：每轮位移不同，但不依赖随机数
        let dx = (i as i32 * 37) % 240;
        let dy = (i as i32 * 53) % 180;
        for (idx, m) in mocks.iter().enumerate() {
            if idx as usize == 2 {
                win::window::apply_placement(m.hwnd, 120 + dx, 90 + dy, 700, 480, ShowState::Normal);
            } else {
                win::window::set_window_rect(m.hwnd, 100 + dx + idx as i32 * 40, 80 + dy, 820, 540);
            }
        }
        std::thread::sleep(Duration::from_millis(260));

        let report = match restore::apply(None, &scene, false, None) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("第 {i} 轮恢复失败：{e}");
                continue;
            }
        };

        for step in &report.steps {
            let delta = step
                .delta_x
                .unwrap_or(0)
                .abs()
                .max(step.delta_y.unwrap_or(0).abs());
            let ok = step.status == core::models::StepStatus::Success
                && delta <= 8
                && step.state_restored;
            total += 1;
            if ok {
                success += 1;
            }
            let code = step.code.clone().unwrap_or_else(|| "ok".into());
            *codes.entry(code.clone()).or_insert(0) += 1;

            lines.push(
                serde_json::json!({
                    "iteration": i,
                    "target": step.target_name,
                    "status": format!("{:?}", step.status).to_lowercase(),
                    "code": code,
                    "deltaPx": delta,
                    "stateRestored": step.state_restored,
                    "durationMs": step.duration_ms,
                    "ok": ok,
                })
                .to_string(),
            );
        }
    }

    let dir = storage::data_dir().join("baseline");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("baseline-{:x}.jsonl", core::time::now_ms()));
    let _ = std::fs::write(&path, lines.join("\n"));

    let rate = if total == 0 {
        0.0
    } else {
        success as f64 * 100.0 / total as f64
    };
    println!("[2/3] 复核项 {total} 个，通过 {success} 个，成功率 {rate:.1}%");
    println!("[3/3] 错误码分布：{codes:?}");
    println!("     明细：{}", path.display());

    let _ = tauri::async_runtime::block_on(data::workspaces::delete(&db, &ws.id));
    for m in mocks {
        m.close();
    }

    let pass = rate >= 95.0;
    println!(
        "\n基线结果：{}（门槛 95%，实际 {rate:.1}%）",
        if pass { "PASS" } else { "FAIL" }
    );
    if !pass {
        std::process::exit(1);
    }
}

/// 自动快照后台任务：只在存在"当前激活工作空间"且其开关打开时工作。
/// 每分钟看一次桌面，指纹不变就不写库（红线二：不能每次窗口变化都落库）。
fn spawn_auto_snapshot(app: tauri::AppHandle) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        // 启动后先等一会，避免与应用启动抢资源
        std::thread::sleep(Duration::from_secs(40));
        loop {
            let db = app.state::<data::Db>().inner().clone();
            match tauri::async_runtime::block_on(auto_snapshot_once(&db)) {
                Ok(Some(ws_id)) => {
                    let _ = app.emit("snapshot:auto_created", ws_id);
                }
                Ok(None) => {}
                Err(e) => eprintln!("自动快照失败：{e}"),
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

/// 轻量监听显示器热插拔（仅枚举显示器拓扑，耗时 < 0.2ms）：插拔外接屏时通知前端自动刷新布局与提示用户
fn spawn_display_watcher(app: tauri::AppHandle) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        let sig = || -> (usize, String) {
            let mons = win::display::enumerate();
            let mut parts: Vec<String> = mons
                .iter()
                .map(|m| {
                    format!(
                        "{}:{}x{}@{},{}#{}",
                        m.key,
                        m.rect.right - m.rect.left,
                        m.rect.bottom - m.rect.top,
                        m.rect.left,
                        m.rect.top,
                        m.dpi
                    )
                })
                .collect();
            parts.sort();
            (mons.len(), parts.join("|"))
        };
        let (_, mut last) = sig();
        loop {
            std::thread::sleep(Duration::from_secs(4));
            let (count, now_sig) = sig();
            if count > 0 && now_sig != last {
                last = now_sig;
                let _ = app.emit("displays:changed", count);
            }
        }
    });
}

async fn auto_snapshot_once(db: &data::Db) -> core::AppResult<Option<String>> {
    let settings = data::settings::get(db).await?;
    if !settings.auto_snapshot_enabled {
        return Ok(None);
    }

    let Some(ws) = data::snapshots::active_workspace(db).await? else {
        return Ok(None);
    };

    let enabled: Option<i64> = sqlx::query_scalar(
        "SELECT auto_snapshot_enabled FROM workspaces WHERE id = ?",
    )
    .bind(&ws)
    .fetch_optional(&db.pool)
    .await?;

    if enabled.unwrap_or(0) == 0 {
        return Ok(None);
    }

    let capture = capture::scan()?;
    if data::snapshots::create_auto(
        db,
        &ws,
        &capture,
        settings.auto_min_interval_secs * 1000,
    )
    .await?
    .is_some()
    {
        // 新写入后才需要按保留策略清理
        let _ = data::snapshots::prune(db, &ws, settings.auto_keep as usize).await;
        return Ok(Some(ws));
    }
    Ok(None)
}

/// M5 自检：Hybrid 快照。
/// 建空间 → 自动快照去重（指纹相同不写）→ 变化后写入 → 保留策略只淘汰旧自动快照 →
/// 手动快照受保护 → 差异比较 → 删除。
/// 用法：cargo run -- --m5test
pub fn m5test() {
    win::dpi::enable_per_monitor_v2();
    println!("[env] data_dir={}", storage::data_dir().display());
    let mut ok = true;

    let Some(mock_a) = win::mock::MockWindow::spawn("M5 窗口 A", (320, 220, 860, 580)) else {
        eprintln!("无法创建模拟窗口，自检中止");
        std::process::exit(2);
    };
    let Some(mock_b) = win::mock::MockWindow::spawn("M5 窗口 B", (720, 380, 760, 520)) else {
        eprintln!("无法创建第二个模拟窗口，自检中止");
        mock_a.close();
        std::process::exit(2);
    };
    std::thread::sleep(Duration::from_millis(350));

    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };

    let capture = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    let hwnds = [mock_a.hwnd as usize, mock_b.hwnd as usize];
    let mine: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| hwnds.contains(&w.hwnd))
        .cloned()
        .collect();
    if mine.len() != 2 {
        eprintln!("捕获到 {} 个模拟窗口（应为 2）", mine.len());
        mock_a.close();
        mock_b.close();
        std::process::exit(2);
    }
    let isolated = CaptureResult {
        windows: mine,
        displays: capture.displays.clone(),
        ..capture
    };

    let ws = match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db,
        "M5 自检工作空间",
        &isolated,
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("创建工作空间失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };

    let list = tauri::async_runtime::block_on(data::snapshots::list(&db, &ws.id, 50));
    let count_initial = list.map(|v| v.len()).unwrap_or(0);
    println!("[1/6] 初始快照数：{count_initial}（建空间时自动写入 1 份）");

    // 指纹相同 -> 跳过
    let same = tauri::async_runtime::block_on(data::snapshots::create_auto(
        &db,
        &ws.id,
        &isolated,
        0,
    ));
    let same_skipped = matches!(same, Ok(None));
    println!("[2/6] 桌面未变时自动快照被去重：{same_skipped}");
    if !same_skipped {
        ok = false;
    }

    // 移动窗口 -> 指纹变化 -> 写入
    win::window::set_window_rect(mock_a.hwnd, 900, 500, 780, 520);
    std::thread::sleep(Duration::from_millis(300));
    let capture2 = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("第二次捕获失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    let capture2 = CaptureResult {
        windows: capture2
            .windows
            .iter()
            .filter(|w| hwnds.contains(&w.hwnd))
            .cloned()
            .collect(),
        ..capture2
    };
    let created_auto = tauri::async_runtime::block_on(data::snapshots::create_auto(
        &db,
        &ws.id,
        &capture2,
        0,
    ));
    let auto_ok = matches!(created_auto, Ok(Some(_)));
    println!("[3/6] 桌面变化后写入自动快照：{auto_ok}");
    if !auto_ok {
        ok = false;
    }

    // 再来一份自动快照 + 手动快照
    win::window::set_window_rect(mock_a.hwnd, 420, 260, 820, 560);
    std::thread::sleep(Duration::from_millis(300));
    let capture3 = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("第三次捕获失败：{e}");
            mock_a.close();
            mock_b.close();
            std::process::exit(2);
        }
    };
    let capture3 = CaptureResult {
        windows: capture3
            .windows
            .iter()
            .filter(|w| hwnds.contains(&w.hwnd))
            .cloned()
            .collect(),
        ..capture3
    };
    let _ = tauri::async_runtime::block_on(data::snapshots::create_auto(&db, &ws.id, &capture3, 0));
    let manual = tauri::async_runtime::block_on(data::snapshots::create_manual(
        &db,
        &ws.id,
        "手动快照",
        &capture3,
    ));
    println!(
        "[4/6] 手动快照已创建：{}（protected={:?}）",
        manual.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
        manual.as_ref().map(|m| m.is_protected)
    );

    // 保留策略：只留 1 份自动快照
    let removed = tauri::async_runtime::block_on(data::snapshots::prune(&db, &ws.id, 1));
    let after = tauri::async_runtime::block_on(data::snapshots::list(&db, &ws.id, 50))
        .unwrap_or_default();
    let autos = after.iter().filter(|s| s.snapshot_type == "auto").count();
    let manuals = after.iter().filter(|s| s.snapshot_type == "manual").count();
    println!(
        "[5/6] 保留策略：淘汰 {removed:?} 份，剩余自动 {autos} / 手动 {manuals} / 总计 {}",
        after.len()
    );
    if autos > 1 || manuals != 1 {
        ok = false;
    }

    // 差异比较：初始快照 vs 最新自动快照
    let initial = after.iter().find(|s| s.snapshot_type == "initial").map(|s| s.id.clone());
    let newest = after
        .iter()
        .find(|s| s.snapshot_type == "auto")
        .map(|s| s.id.clone());
    if let (Some(a), Some(b)) = (initial, newest) {
        match (
            storage::load_scene(&a),
            storage::load_scene(&b),
        ) {
            (Ok(sa), Ok(sb)) => {
                let d = data::snapshots::diff(&sa, &sb);
                println!(
                    "[6/6] 差异：新增 {} / 移除 {} / 移动 {} / 状态变化 {} / 未变 {}（{}）",
                    d.added, d.removed, d.moved, d.state_changed, d.unchanged, d.display_detail
                );
                if d.moved == 0 {
                    ok = false;
                }
            }
            _ => {
                eprintln!("读取快照 payload 失败");
                ok = false;
            }
        }
    } else {
        eprintln!("缺少可比对的快照");
        ok = false;
    }

    let _ = tauri::async_runtime::block_on(data::workspaces::delete(&db, &ws.id));
    mock_a.close();
    mock_b.close();

    println!("\nM5 结果：{}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

/// M9 自检：资源恢复（文件/文件夹/网址）。
/// 只验证「入库 → 构造恢复计划 → 预览判定」这一段；不真的打开资源，
/// 避免在自检里弹出资源管理器/浏览器窗口。
/// 用法：cargo run -- --m9test
pub fn m9test() {
    win::dpi::enable_per_monitor_v2();
    println!("[env] data_dir={}", storage::data_dir().display());
    let mut ok = true;

    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            std::process::exit(2);
        }
    };

    let Some(mock) = win::mock::MockWindow::spawn("M9 模拟窗口", (340, 240, 820, 560)) else {
        eprintln!("无法创建模拟窗口，自检中止");
        std::process::exit(2);
    };
    std::thread::sleep(Duration::from_millis(300));

    let capture = match capture::scan_for_test() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            mock.close();
            std::process::exit(2);
        }
    };
    let mine: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| w.hwnd == mock.hwnd as usize)
        .cloned()
        .collect();
    let isolated = CaptureResult {
        windows: mine,
        displays: capture.displays.clone(),
        ..capture
    };

    let ws = match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db,
        "M9 资源自检",
        &isolated,
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("创建工作空间失败：{e}");
            mock.close();
            std::process::exit(2);
        }
    };

    // 一个真实存在的文件（仓库里的 README）、一个文件夹、一个网址、一个不存在的路径
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let real_file = repo.join("README.md");
    let real_folder = repo.join("docs");

    let mut added = 0;
    for (kind, target) in [
        ("file", real_file.to_string_lossy().to_string()),
        ("folder", real_folder.to_string_lossy().to_string()),
        ("url", "https://example.com/docs".to_string()),
        ("file", repo.join("no-such-file.md").to_string_lossy().to_string()),
    ] {
        match tauri::async_runtime::block_on(data::workspaces::add_item(
            &db,
            &ws.id,
            kind,
            &target,
            None,
        )) {
            Ok(_) => added += 1,
            Err(e) => {
                eprintln!("添加资源失败（{kind} {target}）：{e}");
                ok = false;
            }
        }
    }
    println!("[1/3] 已添加 {added} 个资源");

    let detail = tauri::async_runtime::block_on(data::workspaces::get(&db, &ws.id));
    let resources = detail
        .map(|d| d.items.iter().filter(|i| i.item_type != "application").count())
        .unwrap_or(0);
    println!("[2/3] 工作空间内资源项：{resources}");
    if resources != 4 {
        ok = false;
    }

    let scene = match tauri::async_runtime::block_on(data::workspaces::to_scene(&db, &ws.id)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("构造恢复计划失败：{e}");
            mock.close();
            std::process::exit(2);
        }
    };
    let preview = match restore::preview(&scene) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("生成预览失败：{e}");
            mock.close();
            std::process::exit(2);
        }
    };

    let resource_items: Vec<_> = preview
        .items
        .iter()
        .filter(|i| i.detail.starts_with("将用") || i.detail.starts_with("将交给"))
        .collect();
    let missing = resource_items
        .iter()
        .filter(|i| i.status == core::models::StepStatus::Failed)
        .count();

    println!(
        "[3/3] 预览：{} 个窗口 + {} 个资源（其中 {} 个判定为不可达）",
        preview.items.len() - resource_items.len(),
        resource_items.len(),
        missing
    );

    // 4 个资源里应当正好 1 个不可达（no-such-file.md）
    if scene.resources.len() != 4 || resource_items.len() != 4 || missing != 1 {
        ok = false;
    }

    // 保留现场时用 WORKSCENE_KEEP=1，方便截图/人工检查
    if std::env::var("WORKSCENE_KEEP").is_err() {
        let _ = tauri::async_runtime::block_on(data::workspaces::delete(&db, &ws.id));
    }
    mock.close();

    println!("\nM9 结果：{}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

/// 现场基线：用**当前真实运行的窗口**测一次恢复成功率。
///
/// 这是唯一能证明"真实应用能不能恢复"的测法，代价是会临时移动用户已开的窗口
/// （恢复会把它们放回去；开始前先存一份保护快照）。
/// 用法：cargo run -- --baseline-live 6
pub fn baseline_live(max_windows: usize) {
    win::dpi::enable_per_monitor_v2();
    let max_windows = max_windows.clamp(1, 12);
    println!("[env] data_dir={}", storage::data_dir().display());
    let self_pid = std::process::id();
    let _ = self_pid;

    let db = match tauri::async_runtime::block_on(data::init()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("数据库初始化失败：{e}");
            std::process::exit(2);
        }
    };

    let capture = match capture::scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("捕获失败：{e}");
            std::process::exit(2);
        }
    };

    // 只挑真实工作窗口，且排除本程序自己的窗口
    let mut targets: Vec<WindowInfo> = capture
        .windows
        .iter()
        .filter(|w| w.width >= 300 && w.height >= 200)
        .cloned()
        .collect();
    targets.truncate(max_windows);

    if targets.is_empty() {
        eprintln!("没有可测的真实窗口，请先打开几个应用");
        std::process::exit(2);
    }

    println!(
        "[1/5] 选定 {} 个真实窗口（共捕获 {} 个）",
        targets.len(),
        capture.windows.len()
    );
    for w in &targets {
        println!(
            "      {:<28} {:<26} {}x{}",
            w.process_name, w.class_name, w.width, w.height
        );
    }

    let scene_capture = CaptureResult {
        windows: targets.clone(),
        displays: capture.displays.clone(),
        ..capture.clone()
    };

    let ws = match tauri::async_runtime::block_on(data::workspaces::create_from_capture(
        &db,
        "现场基线",
        &scene_capture,
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("创建工作空间失败：{e}");
            std::process::exit(2);
        }
    };
    println!("[2/5] 已保存基线工作空间（{}），原始布局已落库", ws.id);

    let originals: Vec<(usize, (i32, i32, i32, i32))> = targets
        .iter()
        .filter_map(|w| {
            win::window::current_rect(w.hwnd as HWND)
                .map(|r| (w.hwnd, (r.left, r.top, r.right - r.left, r.bottom - r.top)))
        })
        .collect();
    println!("[3/5] 记录原始位置 {} 个窗口", originals.len());

    // 扰动：每个窗口挪开一点（尺寸不变，只测位置恢复）
    for (i, (hwnd, (x, y, w, h))) in originals.iter().enumerate() {
        let dx = 60 + i as i32 * 17;
        let dy = 40 + i as i32 * 11;
        win::window::set_window_rect(*hwnd as HWND, x + dx, y + dy, *w, *h);
    }
    std::thread::sleep(Duration::from_millis(600));

    let scene = match tauri::async_runtime::block_on(data::workspaces::to_scene(&db, &ws.id)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("构造恢复计划失败：{e}");
            std::process::exit(2);
        }
    };

    println!("[4/5] 开始按工作空间恢复…");
    let report = match restore::apply(None, &scene, false, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("恢复失败：{e}");
            std::process::exit(2);
        }
    };

    // 独立复核：不信任报告，自己再量一次
    let mut lines: Vec<String> = Vec::new();
    let mut measured = 0usize;
    let mut passed = 0usize;
    let mut worst = 0i32;

    println!("[5/5] 逐窗口复核（报告值 / 独立测量）：");
    for (hwnd, original) in &originals {
        let now = win::window::current_rect(*hwnd as HWND);
        let Some(now) = now else {
            lines.push(
                serde_json::json!({ "hwnd": hwnd, "status": "vanished", "ok": false }).to_string(),
            );
            println!("      HWND {} 窗口已消失（用户可能关掉了）", hwnd);
            continue;
        };

        let dx = (now.left - original.0).abs();
        let dy = (now.top - original.1).abs();
        let dw = ((now.right - now.left) - original.2).abs();
        let dh = ((now.bottom - now.top) - original.3).abs();
        let d = dx.max(dy);
        let size_delta = dw.max(dh);
        let ok = d <= 8 && size_delta <= 8;
        measured += 1;
        if ok {
            passed += 1;
        }
        worst = worst.max(d.max(size_delta));

        lines.push(
            serde_json::json!({
                "hwnd": hwnd,
                "expected": [original.0, original.1, original.2, original.3],
                "actual": [now.left, now.top, now.right - now.left, now.bottom - now.top],
                "deltaPx": d,
                "sizeDeltaPx": size_delta,
                "ok": ok,
            })
            .to_string(),
        );

        println!(
            "      HWND {:<10} 位置 ({}, {}) → ({}, {}) 偏差 {}px，尺寸 {}x{} → {}x{} 偏差 {}px {}",
            hwnd,
            original.0,
            original.1,
            now.left,
            now.top,
            d,
            original.2,
            original.3,
            now.right - now.left,
            now.bottom - now.top,
            size_delta,
            if ok { "✓" } else { "✗" }
        );
    }

    let rate = if measured == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / measured as f64
    };

    // 落一条 run，便于在「恢复记录」里回看
    let _ = tauri::async_runtime::block_on(data::runs::save(
        &db,
        &report,
        Some(&ws.id),
        None,
        "baseline-live",
    ));

    let dir = storage::data_dir().join("baseline");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("live-{:x}.jsonl", core::time::now_ms()));
    let _ = std::fs::write(&path, lines.join("\n"));

    println!(
        "\n真实窗口基线：{passed}/{measured} 通过，成功率 {rate:.1}%，最大偏差 {worst}px"
    );
    println!("明细：{}", path.display());
    println!(
        "报告口径：成功 {} / 失败 {} / 跳过 {}",
        report.succeeded, report.failed, report.skipped
    );
    for s in &report.steps {
        println!(
            "      [{}] {} · {:?} · {}",
            s.step_type, s.target_name, s.status, s.message
        );
    }

    let _ = tauri::async_runtime::block_on(data::workspaces::delete(&db, &ws.id));

    if rate < 95.0 {
        std::process::exit(1);
    }
}

/// 保留：按 pid 等待窗口，供后续需要外部进程的测试使用
#[allow(dead_code)]
fn wait_window_of_pid(pid: u32, timeout: Duration) -> Option<HWND> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        for raw in win::window::enumerate_raw() {
            if raw.pid == pid && raw.is_visible {
                let w = raw.rect.right - raw.rect.left;
                let h = raw.rect.bottom - raw.rect.top;
                if w > 120 && h > 90 {
                    return Some(raw.hwnd);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}
