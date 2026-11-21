mod capture;
mod commands;
mod core;
mod restore;
mod storage;
mod win;

use core::models::{CaptureResult, ShowState, WindowInfo};
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;

pub fn run() {
    // 必须在创建任何窗口前设置，否则坐标会被 DWM 虚拟化
    win::dpi::enable_per_monitor_v2();

    tauri::Builder::default()
        .setup(|app| {
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
            commands::scene_list,
            commands::scene_save,
            commands::scene_get,
            commands::scene_delete,
            commands::scene_health,
            commands::run_list,
            commands::restore_preview,
            commands::restore_apply,
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

/// 无头捕获：把当前桌面直接保存为一份快照（可脚本化、可做回归基线）。
/// 用法：cargo run -- --capture "前端开发"
pub fn cli_capture(name: &str) {
    win::dpi::enable_per_monitor_v2();
    match capture::scan() {
        Ok(result) => match storage::save_scene(name, &result) {
            Ok(scene) => println!(
                "已保存：{}（{} 个窗口 / {} 台显示器）→ {}",
                scene.name, scene.window_count, scene.display_count, scene.path
            ),
            Err(e) => eprintln!("保存失败：{e}"),
        },
        Err(e) => eprintln!("捕获失败：{e}"),
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
