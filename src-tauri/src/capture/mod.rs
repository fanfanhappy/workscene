pub mod context;

use crate::core::models::{
    CaptureResult, DisplayInfo, ExcludedWindow, ShowState, WindowInfo,
};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::win::{display, dpi, filter, process, window};
use std::collections::HashMap;

/// 仅枚举显示器拓扑，不扫描窗口与进程（毫秒级返回）。
pub fn displays() -> AppResult<Vec<DisplayInfo>> {
    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Err(AppError::msg("未检测到显示器"));
    }
    Ok(monitors_to_info(&monitors))
}

fn monitors_to_info(monitors: &[display::RawMonitor]) -> Vec<DisplayInfo> {
    monitors
        .iter()
        .map(|m| DisplayInfo {
            monitor_key: m.key.clone(),
            device_name: m.device_name.clone(),
            x: m.rect.left,
            y: m.rect.top,
            width: m.rect.right - m.rect.left,
            height: m.rect.bottom - m.rect.top,
            work_x: m.work.left,
            work_y: m.work.top,
            work_width: m.work.right - m.work.left,
            work_height: m.work.bottom - m.work.top,
            dpi: m.dpi,
            scale_factor: display::scale_factor(m.dpi),
            is_primary: m.is_primary,
        })
        .collect()
}

/// 扫描当前桌面：显示器 + 可捕获窗口 + 排除项诊断。
/// 纯采集，不写入任何存储；用户确认后才保存为快照（文档 §3.2）。
pub fn scan() -> AppResult<CaptureResult> {
    scan_internal(false)
}

/// 自检模式：允许保留与 WorkScene 同进程的模拟窗口（见 win::mock）。
pub fn scan_for_test() -> AppResult<CaptureResult> {
    scan_internal(true)
}

fn scan_internal(allow_self_window: bool) -> AppResult<CaptureResult> {
    let started = std::time::Instant::now();
    let self_pid = std::process::id();

    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Err(AppError::msg("未检测到显示器，无法捕获桌面"));
    }

    let displays = monitors_to_info(&monitors);

    let (raw_windows, fast_skipped) =
        window::enumerate_visible(if allow_self_window { None } else { Some(self_pid) });

    let mut windows: Vec<WindowInfo> = Vec::with_capacity(raw_windows.len());
    let mut excluded: Vec<ExcludedWindow> = Vec::with_capacity(24);
    let mut excluded_total = fast_skipped;
    let mut protected_processes = 0usize;
    let mut pid_cache: HashMap<u32, process::ProcessInfo> = HashMap::with_capacity(32);

    for raw in raw_windows {
        if let Some(reason) = filter::exclude_reason(&raw, self_pid, allow_self_window) {
            excluded_total += 1;
            // 只保留有标题的可见排除项，且复用 PID 缓存避免重复 OpenProcess
            if excluded.len() < 120 && !raw.title.trim().is_empty() {
                let pinfo = pid_cache
                    .entry(raw.pid)
                    .or_insert_with(|| process::info(raw.pid));
                excluded.push(ExcludedWindow {
                    title: raw.title,
                    class_name: raw.class_name,
                    process_name: pinfo.name.clone(),
                    reason,
                });
            }
            continue;
        }

        let pinfo = pid_cache
            .entry(raw.pid)
            .or_insert_with(|| process::info(raw.pid));

        // 尊重用户在「设置 -> 应用适配」里配置的全局捕获忽略名单（不静默丢弃，透明记入 excluded）
        if !allow_self_window && filter::is_ignored_process(&pinfo.name) {
            excluded_total += 1;
            if excluded.len() < 120 {
                excluded.push(ExcludedWindow {
                    title: raw.title,
                    class_name: raw.class_name,
                    process_name: pinfo.name.clone(),
                    reason: "已在全局忽略名单中".into(),
                });
            }
            continue;
        }

        // 最小化窗口的 GetWindowRect 不可用；最大化窗口的 GetWindowRect 是含隐形边的整屏矩形，
        // 两者都优先用 rcNormalPosition 作为还原矩形，避免取消最大化后窗口卡在整屏尺寸
        let restored_valid = raw.restored_rect.right > raw.restored_rect.left
            && raw.restored_rect.bottom > raw.restored_rect.top;
        let effective = if raw.show_state != ShowState::Normal && restored_valid {
            raw.restored_rect
        } else {
            // 普通窗口优先用 DWM 真实可见边界，消除 Win10/11 约 7px 透明阴影边框导致的贴边缝隙
            window::extended_frame_bounds(raw.hwnd).unwrap_or(raw.rect)
        };

        let mon = display::monitor_for_rect(&effective, &monitors);
        let window_dpi = dpi::window_dpi(raw.hwnd);

        if pinfo.path.is_none() {
            protected_processes += 1;
        }

        windows.push(WindowInfo {
            hwnd: raw.hwnd as usize,
            process_id: raw.pid,
            process_path: pinfo.path.clone(),
            process_name: pinfo.name.clone(),
            process_started_at: pinfo.started_at,
            title: raw.title,
            class_name: raw.class_name,
            x: effective.left,
            y: effective.top,
            width: effective.right - effective.left,
            height: effective.bottom - effective.top,
            dpi: window_dpi,
            monitor_key: mon.map(|m| m.key.clone()),
            is_visible: raw.is_visible,
            is_minimized: raw.show_state == ShowState::Minimized,
            is_maximized: raw.show_state == ShowState::Maximized,
            show_state: raw.show_state,
        });
    }

    // EnumWindows 返回顺序即 z-order（最上层在前），window_index 直接作为层序写入快照
    let mut diagnostics = Vec::new();
    diagnostics.push(format!(
        "枚举顶层窗口 {} 个，保留 {} 个，排除 {} 个",
        windows.len() + excluded_total,
        windows.len(),
        excluded_total
    ));
    diagnostics.push(format!("显示器 {} 个", displays.len()));
    for m in &monitors {
        diagnostics.push(format!(
            "{} {}×{} @ {}% (DPI {})",
            if m.is_primary { "主屏" } else { "副屏" },
            m.rect.right - m.rect.left,
            m.rect.bottom - m.rect.top,
            (display::scale_factor(m.dpi) * 100.0).round() as i32,
            m.dpi
        ));
    }
    if protected_processes > 0 {
        diagnostics.push(format!(
            "{protected_processes} 个窗口以管理员/受保护权限运行，无法读取可执行路径（恢复时将跳过自动启动）"
        ));
    }

    Ok(CaptureResult {
        captured_at: now_ms(),
        duration_ms: started.elapsed().as_millis() as i64,
        windows,
        displays,
        excluded,
        diagnostics,
    })
}
