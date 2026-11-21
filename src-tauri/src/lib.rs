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

    tauri::Builder::default()
        .setup(|app| {
            // 数据库在启动时初始化（建库 + 跑迁移），失败直接让启动失败，
            // 避免出现"能开窗但什么都不能用"的半死状态。
            let db = tauri::async_runtime::block_on(data::init())?;
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
            commands::workspace_preview,
            commands::workspace_restore,
            commands::snapshot_restore,
            commands::run_list,
            commands::run_error_stats,
        ])
        .run(tauri::generate_context!())
        .expect("WorkScene 启动失败");
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
