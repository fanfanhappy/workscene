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
                    .inner_size(1280.0, 840.0)
                    .min_inner_size(1000.0, 640.0)
                    .center()
                    .theme(Some(tauri::Theme::Dark));

            #[cfg(target_os = "windows")]
            {
                builder = builder.data_directory(webview_dir);
            }

            builder.build()?;

            win::tray::build_tray(app)?;
            win::tray::intercept_close(app);
            win::tray::spawn_hotkey(app.handle().clone());

            spawn_auto_snapshot(app.handle().clone());

            // 诊断用：WORKSCENE_ROUTE=/workspaces 可直接打开指定页面（截图与回归用）
            if let Ok(route) = std::env::var("WORKSCENE_ROUTE") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.eval(&format!(
                        "window.location.hash = '{}';",
                        route.replace('\'', "")
                    ));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system_get_info,
            commands::system_get_displays,
            commands::capture_start,
            commands::scene_health,
            commands::workspace_list,
            commands::workspace_get,
            commands::workspace_create,
            commands::workspace_rename,
            commands::workspace_delete,
            commands::workspace_item_update,
            commands::workspace_layout_update,
            commands::workspace_window_delete,
            commands::workspace_item_delete,
            commands::workspace_preview,
            commands::workspace_restore,
            commands::snapshot_restore,
            commands::run_list,
            commands::run_error_stats,
            commands::restore_is_busy,
            commands::snapshot_list,
            commands::snapshot_create,
            commands::snapshot_delete,
            commands::snapshot_diff,
            commands::snapshot_prune,
            commands::recovery_hint,
            commands::recovery_dismiss,
            commands::adapter_suggest,
        ])
        .build(tauri::generate_context!())
        .expect("WorkScene 构建失败")
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
    std::thread::spawn(move || {
        // 启动后先等一会，避免与应用启动抢资源
        std::thread::sleep(Duration::from_secs(40));
        loop {
            let db = app.state::<data::Db>().inner().clone();
            match tauri::async_runtime::block_on(auto_snapshot_once(&db)) {
                Ok(()) => {}
                Err(e) => eprintln!("自动快照失败：{e}"),
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

async fn auto_snapshot_once(db: &data::Db) -> core::AppResult<()> {
    let Some(ws) = data::snapshots::active_workspace(db).await? else {
        return Ok(());
    };

    let enabled: Option<i64> = sqlx::query_scalar(
        "SELECT auto_snapshot_enabled FROM workspaces WHERE id = ?",
    )
    .bind(&ws)
    .fetch_optional(&db.pool)
    .await?;

    if enabled.unwrap_or(0) == 0 {
        return Ok(());
    }

    let capture = capture::scan()?;
    let _ = data::snapshots::create_auto(
        db,
        &ws,
        &capture,
        data::snapshots::AUTO_MIN_INTERVAL_MS,
    )
    .await?;
    Ok(())
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
