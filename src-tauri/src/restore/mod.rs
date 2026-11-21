pub mod layout;
pub mod matcher;

use crate::core::models::{
    PreviewItem, PreviewSummary, RestorePreview, RestoreReport, RestoreStepResult, SceneFile,
    SceneWindow, ShowState, StepStatus,
};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::win::{display, filter, process, window};
use layout::{clamp, from_ratios, round, Area};
use matcher::{match_window, Candidate};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::HWND;

/// 同 DPI 下的位置/尺寸容差；跨 DPI 时按 max(8, 2×scale) 放大（评审 R2-3）
const BASE_DELTA_PX: i32 = 8;
/// 启动应用后等待窗口出现的最长时间
const LAUNCH_WAIT_MS: u64 = 8000;
/// 单次恢复最多为同一个可执行文件补启的实例数，避免失控
const MAX_RELAUNCH_PER_EXE: usize = 3;

fn current_candidates(self_pid: u32) -> Vec<Candidate> {
    let mut out = Vec::with_capacity(48);
    for raw in window::enumerate_raw() {
        // 允许同进程窗口进入候选：匹配仍要求「可执行路径 + 窗口类名」一致，
        // 因此不会误判到 WorkScene 自己的界面；模拟窗口（win::mock）也才可被恢复测试。
        if filter::exclude_reason(&raw, self_pid, true).is_some() {
            continue;
        }
        let pinfo = process::info(raw.pid);
        let effective = if raw.show_state == ShowState::Minimized {
            raw.restored_rect
        } else {
            raw.rect
        };
        out.push(Candidate {
            hwnd: raw.hwnd as usize,
            pid: raw.pid,
            process_path: pinfo.path,
            process_name: pinfo.name,
            class_name: raw.class_name.clone(),
            title: raw.title.clone(),
            rect: (
                effective.left,
                effective.top,
                effective.right - effective.left,
                effective.bottom - effective.top,
            ),
        });
    }
    out
}

/// 一次恢复里某个窗口的目标位置（显示器 + 工作区像素矩形）
struct Target {
    rect: (i32, i32, i32, i32),
    dpi: u32,
    monitor_rect: (i32, i32, i32, i32),
    note: Option<String>,
}

/// 目标显示器解析：先按设备名匹配，缺失时回退主屏并如实记录
fn resolve_target(win: &SceneWindow, monitors: &[display::RawMonitor]) -> Target {
    let by_key = monitors.iter().find(|m| m.key == win.monitor_key);
    let primary = monitors
        .iter()
        .find(|m| m.is_primary)
        .or_else(|| monitors.first());

    let (m, note) = match (by_key, primary) {
        (Some(m), _) => (m, None),
        (None, Some(p)) => (
            p,
            Some(format!(
                "目标显示器「{}」已不在当前环境，已映射到 {}",
                if win.monitor_key.is_empty() {
                    "未知显示器"
                } else {
                    &win.monitor_key
                },
                p.key
            )),
        ),
        (None, None) => {
            return Target {
                rect: (0, 0, 1280, 720),
                dpi: 96,
                monitor_rect: (0, 0, 1280, 720),
                note: Some("无可用显示器".into()),
            }
        }
    };

    let work = Area::new(
        m.work.left as f64,
        m.work.top as f64,
        (m.work.right - m.work.left) as f64,
        (m.work.bottom - m.work.top) as f64,
    );
    let area = clamp(
        from_ratios(
            (win.x_ratio, win.y_ratio, win.width_ratio, win.height_ratio),
            work,
        ),
        work,
    );

    Target {
        rect: round(area),
        dpi: m.dpi,
        monitor_rect: (
            m.rect.left,
            m.rect.top,
            m.rect.right - m.rect.left,
            m.rect.bottom - m.rect.top,
        ),
        note,
    }
}

pub fn preview(scene: &SceneFile) -> AppResult<RestorePreview> {
    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Err(AppError::msg("未检测到显示器"));
    }
    let self_pid = std::process::id();
    let current = current_candidates(self_pid);
    let mut used: HashSet<usize> = HashSet::new();

    let mut display_notes: Vec<String> = Vec::new();
    let mut items: Vec<PreviewItem> = Vec::with_capacity(scene.windows.len());
    let mut ready = 0;
    let mut warning = 0;
    let mut missing = 0;

    for win in &scene.windows {
        let target = resolve_target(win, &monitors);
        if let Some(n) = &target.note {
            if !display_notes.contains(n) {
                display_notes.push(n.clone());
            }
        }
        let rect = target.rect;

        let pool: Vec<Candidate> = current
            .iter()
            .filter(|c| !used.contains(&c.hwnd))
            .cloned()
            .collect();

        let exe_exists = win
            .process_path
            .as_ref()
            .map(|p| std::path::Path::new(p).is_file())
            .unwrap_or(false);

        let outcome = match_window(win, &pool, rect);
        let target_name = if win.title.trim().is_empty() {
            win.process_name.clone()
        } else {
            win.title.clone()
        };

        let item = if let Some(idx) = outcome.picked {
            used.insert(pool[idx].hwnd);
            ready += 1;
            PreviewItem {
                target_name,
                detail: format!(
                    "将调整到 {} · ({}, {}) {}×{}",
                    win.monitor_key, rect.0, rect.1, rect.2, rect.3
                ),
                status: StepStatus::Success,
                match_note: format!("{} → PID {}", outcome.note, pool[idx].pid),
            }
        } else if outcome.ambiguous {
            warning += 1;
            PreviewItem {
                target_name,
                detail: "存在多个评分接近的候选窗口，恢复时将跳过".into(),
                status: StepStatus::NeedsReview,
                match_note: outcome.note,
            }
        } else if !exe_exists && !win.process_name.is_empty() {
            let running = current
                .iter()
                .any(|c| c.process_name.eq_ignore_ascii_case(&win.process_name));
            if running {
                warning += 1;
                PreviewItem {
                    target_name,
                    detail: "应用在运行，但可用窗口已被其他目标占用".into(),
                    status: StepStatus::NeedsReview,
                    match_note: outcome.note,
                }
            } else {
                missing += 1;
                PreviewItem {
                    target_name,
                    detail: "应用未运行，且原始路径已不存在".into(),
                    status: StepStatus::Failed,
                    match_note: outcome.note,
                }
            }
        } else {
            warning += 1;
            PreviewItem {
                target_name,
                detail: "应用未运行；恢复时可选择自动启动".into(),
                status: StepStatus::NeedsReview,
                match_note: outcome.note,
            }
        };

        items.push(item);
    }

    Ok(RestorePreview {
        scene_id: scene.id.clone(),
        scene_name: scene.name.clone(),
        summary: PreviewSummary {
            total: items.len(),
            ready,
            warning,
            missing,
        },
        items,
        display_notes,
    })
}

/// 串行恢复：M1 只验证「识别 + 定位」的准确性，并发编排留给 M3。
pub fn apply(
    app: Option<&AppHandle>,
    scene: &SceneFile,
    relaunch_missing: bool,
    pre_restore_scene_id: Option<String>,
) -> AppResult<RestoreReport> {
    let started_at = now_ms();
    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Err(AppError::msg("未检测到显示器"));
    }
    let self_pid = std::process::id();

    let mut steps: Vec<RestoreStepResult> = Vec::with_capacity(scene.windows.len());
    let mut display_notes: Vec<String> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();
    let mut current = current_candidates(self_pid);

    // 先按可执行文件补齐实例数：快照里有 3 个 Chrome 窗口、当前只开 1 个时，
    // 必须再启动 2 个，否则多个目标会抢同一个窗口（评审补充意见 D3）
    if relaunch_missing {
        let mut launched_total: usize = 0;
        for (exe, need) in required_instances(scene) {
            if launched_total >= MAX_RELAUNCH_PER_EXE * 4 {
                break;
            }
            let available = current
                .iter()
                .filter(|c| {
                    c.process_path
                        .as_ref()
                        .map(|p| p.eq_ignore_ascii_case(&exe))
                        .unwrap_or(false)
                })
                .count();
            let missing = need.saturating_sub(available);
            if missing == 0 || !std::path::Path::new(&exe).is_file() {
                continue;
            }
            for _ in 0..missing.min(MAX_RELAUNCH_PER_EXE) {
                launch_app(&exe);
                launched_total += 1;
                if wait_for_process_window(&exe) {
                    current = current_candidates(self_pid);
                }
            }
        }
    }

    // 自底向上恢复，避免上层窗口在移动过程中反复遮挡
    let mut ordered: Vec<&SceneWindow> = scene.windows.iter().collect();
    ordered.sort_by_key(|w| -w.z_order);

    for win in ordered {
        let t0 = Instant::now();
        let target_name = if win.title.trim().is_empty() {
            win.process_name.clone()
        } else {
            win.title.clone()
        };

        let target_info = resolve_target(win, &monitors);
        if let Some(n) = &target_info.note {
            if !display_notes.contains(n) {
                display_notes.push(n.clone());
            }
        }
        let rect = target_info.rect;
        let threshold = delta_threshold(target_info.dpi);

        let pool: Vec<Candidate> = current
            .iter()
            .filter(|c| !used.contains(&c.hwnd))
            .cloned()
            .collect();

        let outcome = match_window(win, &pool, rect);

        let step = if let Some(idx) = outcome.picked {
            let target = &pool[idx];
            used.insert(target.hwnd);
            let hwnd = target.hwnd as HWND;

            let (placed, state_restored) = place_window(hwnd, rect, win.show_state);
            let elapsed = || t0.elapsed().as_millis() as i64;

            if !placed {
                RestoreStepResult {
                    target_name,
                    status: StepStatus::Failed,
                    message: "SetWindowPlacement 调用失败（窗口已关闭或受 UIPI 限制）".into(),
                    match_note: outcome.note,
                    duration_ms: elapsed(),
                    delta_x: None,
                    delta_y: None,
                    state_restored: false,
                    code: Some("window_control_denied".into()),
                }
            } else {
                // 等应用自身把窗口改回去（Chromium/Electron 常见），再判定偏差
                std::thread::sleep(Duration::from_millis(160));
                let mut delta = verify(hwnd, &target_info, win.show_state);
                if delta.0 > threshold {
                    // 二次校正
                    place_window(hwnd, rect, win.show_state);
                    std::thread::sleep(Duration::from_millis(200));
                    delta = verify(hwnd, &target_info, win.show_state);
                }

                let position_ok = delta.0 <= threshold;
                let maximized_failed =
                    win.show_state == ShowState::Maximized && !state_restored;

                let status = if position_ok && !maximized_failed {
                    StepStatus::Success
                } else {
                    StepStatus::Failed
                };

                let message = if !position_ok {
                    format!(
                        "窗口被应用自身改回原位置（偏差 {}px，阈值 {threshold}px），已尝试二次校正",
                        delta.0
                    )
                } else if maximized_failed {
                    "位置已恢复，但窗口未能保持最大化".into()
                } else if win.show_state == ShowState::Minimized {
                    "已恢复到目标位置；原快照中该窗口为最小化，为避免误解已还原为普通窗口".into()
                } else if win.show_state == ShowState::Maximized {
                    "已最大化到目标显示器".into()
                } else {
                    format!("已恢复，位置/尺寸偏差 {}px", delta.0)
                };

                let code = if !position_ok {
                    Some("layout_drift".to_string())
                } else if maximized_failed {
                    Some("state_not_restored".to_string())
                } else if win.show_state == ShowState::Minimized {
                    Some("state_not_restored".to_string())
                } else {
                    None
                };

                RestoreStepResult {
                    target_name,
                    status,
                    message,
                    match_note: outcome.note,
                    duration_ms: elapsed(),
                    delta_x: Some(delta.1),
                    delta_y: Some(delta.2),
                    state_restored,
                    code,
                }
            }
        } else if outcome.ambiguous {
            RestoreStepResult {
                target_name,
                status: StepStatus::Skipped,
                message: "窗口匹配存在歧义，已跳过（不改动任何窗口）".into(),
                match_note: outcome.note,
                duration_ms: t0.elapsed().as_millis() as i64,
                delta_x: None,
                delta_y: None,
                state_restored: false,
                code: Some("match_ambiguous".into()),
            }
        } else {
            RestoreStepResult {
                target_name,
                status: StepStatus::Skipped,
                message: "未找到可用的对应窗口（应用未运行，或已被其他目标占用）".into(),
                match_note: outcome.note,
                duration_ms: t0.elapsed().as_millis() as i64,
                delta_x: None,
                delta_y: None,
                state_restored: false,
                code: Some("window_not_found".into()),
            }
        };

        // 真实进度：每完成一步推送一次（无 UI 的自检里 app 为 None）
        if let Some(app) = app {
            let _ = app.emit("restore:step", &step);
        }
        steps.push(step);
    }

    let succeeded = steps.iter().filter(|s| s.status == StepStatus::Success).count();
    let failed = steps.iter().filter(|s| s.status == StepStatus::Failed).count();
    let skipped = steps
        .iter()
        .filter(|s| matches!(s.status, StepStatus::Skipped | StepStatus::NeedsReview))
        .count();

    let report = RestoreReport {
        run_id: format!("run-{:x}", started_at),
        scene_id: scene.id.clone(),
        scene_name: scene.name.clone(),
        started_at,
        finished_at: now_ms(),
        total_steps: steps.len(),
        succeeded,
        failed,
        skipped,
        steps,
        display_notes,
        pre_restore_scene_id,
    };

    // 报告由调用方（命令层）写入数据库，恢复引擎只负责执行与产出报告
    Ok(report)
}

/// 快照里每个可执行文件需要几个窗口实例
fn required_instances(scene: &SceneFile) -> Vec<(String, usize)> {
    let mut map: Vec<(String, usize)> = Vec::new();
    for w in &scene.windows {
        let Some(path) = w.process_path.as_ref() else {
            continue;
        };
        match map.iter_mut().find(|(p, _)| p.eq_ignore_ascii_case(path)) {
            Some((_, n)) => *n += 1,
            None => map.push((path.clone(), 1)),
        }
    }
    map
}

fn delta_threshold(dpi: u32) -> i32 {
    let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
    BASE_DELTA_PX.max((2.0 * scale).round() as i32)
}

/// 恢复窗口位置与显示状态，返回 (写入是否成功, 显示状态是否与快照一致)
fn place_window(hwnd: HWND, rect: (i32, i32, i32, i32), target_state: ShowState) -> (bool, bool) {
    if !window::is_window(hwnd) {
        return (false, false);
    }

    let placed = match target_state {
        ShowState::Maximized => {
            // 先给一个还原矩形，再最大化；否则最大化会回到旧屏幕
            window::set_window_rect(hwnd, rect.0, rect.1, rect.2, rect.3);
            window::apply_placement(hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Maximized)
        }
        // 最小化不强行还原为最小化：用户看不到窗口会以为失败（但报告会说明）
        ShowState::Minimized | ShowState::Normal => {
            window::apply_placement(hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Normal)
        }
    };

    if !placed {
        return (false, false);
    }

    let state_restored = match target_state {
        ShowState::Maximized => window::is_zoomed(hwnd),
        ShowState::Normal => !window::is_zoomed(hwnd) && !window::is_iconic(hwnd),
        ShowState::Minimized => false,
    };

    (true, state_restored)
}

/// 四边偏差：返回 (最大偏差, dx, dy)，尺寸偏差计入最大值（评审补充意见 D4）。
///
/// 最大化窗口的 GetWindowRect 返回的是整块显示器（含不可见边框），
/// 不能拿它和还原矩形比较，否则会把成功的最大化判成失败；
/// 这种情况下只校验「窗口中心是否落在目标显示器内」+ 是否真的处于最大化。
fn verify(hwnd: HWND, target: &Target, state: ShowState) -> (i32, i32, i32) {
    let Some(r) = window::current_rect(hwnd) else {
        return (i32::MAX, 0, 0);
    };

    if state == ShowState::Maximized {
        let cx = (r.left + r.right) / 2;
        let cy = (r.top + r.bottom) / 2;
        let (mx, my, mw, mh) = target.monitor_rect;
        let inside = cx >= mx && cx < mx + mw && cy >= my && cy < my + mh;
        return (if inside { 0 } else { i32::MAX }, 0, 0);
    }

    let dx = (r.left - target.rect.0).abs();
    let dy = (r.top - target.rect.1).abs();
    let dw = ((r.right - r.left) - target.rect.2).abs();
    let dh = ((r.bottom - r.top) - target.rect.3).abs();
    (dx.max(dy).max(dw).max(dh), dx, dy)
}

fn launch_app(exe: &str) {
    let mut cmd = std::process::Command::new(exe);
    if let Some(dir) = std::path::Path::new(exe).parent() {
        cmd.current_dir(dir);
    }
    // 不拼接 Shell 字符串，参数始终以数组传入（文档 §14.2）
    let _ = cmd.spawn();
}

fn wait_for_process_window(exe: &str) -> bool {
    let deadline = Instant::now() + Duration::from_millis(LAUNCH_WAIT_MS);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(120));
        for raw in window::enumerate_raw() {
            if !raw.is_visible {
                continue;
            }
            let pinfo = process::info(raw.pid);
            if let Some(p) = pinfo.path {
                if p.eq_ignore_ascii_case(exe) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_never_drops_below_base() {
        assert_eq!(delta_threshold(96), 8);
        assert_eq!(delta_threshold(144), 8);
        assert_eq!(delta_threshold(480), 10);
    }
}
