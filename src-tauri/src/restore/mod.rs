pub mod layout;
pub mod matcher;

use crate::core::models::{
    PreviewItem, PreviewSummary, RestorePreview, RestoreReport, RestoreStepResult, SceneFile,
    SceneResource, SceneWindow, ShowState, StepStatus,
};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::win::{display, filter, process, window};
use layout::{clamp, clamp_visible, from_ratios, round, Area};
use matcher::{match_window, match_window_for_close, Candidate};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::HWND;

/// 记录每个工作空间最近一次恢复时命中/归属的窗口句柄，供「一键收工」优先精准关闭
static RESTORED_HWNDS: OnceLock<Mutex<HashMap<String, Vec<usize>>>> = OnceLock::new();

fn remember_workspace_hwnds(workspace_id: &str, hwnds: Vec<usize>) {
    let map = RESTORED_HWNDS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = map.lock() {
        guard.insert(workspace_id.to_string(), hwnds);
    }
}

fn take_workspace_hwnds(workspace_id: &str) -> Vec<usize> {
    let map = RESTORED_HWNDS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .ok()
        .and_then(|mut g| g.remove(workspace_id))
        .unwrap_or_default()
}

/// 同 DPI 下的位置/尺寸容差；跨 DPI 时按 max(8, 2×scale) 放大（评审 R2-3）
const BASE_DELTA_PX: i32 = 8;
/// 启动应用后等待窗口出现的最长时间
const LAUNCH_WAIT_MS: u64 = 8000;
/// 单次恢复最多为同一个可执行文件补启的实例数，避免失控
const MAX_RELAUNCH_PER_EXE: usize = 3;
/// 单次恢复总启动上限
/// 单次恢复总启动上限
const MAX_LAUNCH_TOTAL: usize = 8;

/// 全局恢复互斥：两个恢复任务同时跑会互相搬窗口，报告也会互相污染
static RESTORE_BUSY: AtomicBool = AtomicBool::new(false);
/// 取消标记：取消正在执行的恢复任务，但不强制关闭已启动的用户应用（PRD §10.3）
static RESTORE_CANCEL: AtomicBool = AtomicBool::new(false);

pub struct RestoreGuard;

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        RESTORE_CANCEL.store(false, Ordering::SeqCst);
        RESTORE_BUSY.store(false, Ordering::SeqCst);
    }
}

/// 拿不到锁就立刻返回 None，由调用方转成用户可读的错误
pub fn try_acquire() -> Option<RestoreGuard> {
    if RESTORE_BUSY.swap(true, Ordering::SeqCst) {
        None
    } else {
        RESTORE_CANCEL.store(false, Ordering::SeqCst);
        Some(RestoreGuard)
    }
}

pub fn is_busy() -> bool {
    RESTORE_BUSY.load(Ordering::SeqCst)
}

/// 请求取消当前运行中的恢复任务；若当前无运行任务则返回 false
pub fn request_cancel() -> bool {
    if !RESTORE_BUSY.load(Ordering::SeqCst) {
        return false;
    }
    RESTORE_CANCEL.store(true, Ordering::SeqCst);
    true
}

fn is_cancelled() -> bool {
    RESTORE_CANCEL.load(Ordering::SeqCst)
}

fn current_candidates(self_pid: u32) -> Vec<Candidate> {
    let mut out = Vec::with_capacity(48);
    let mut pid_cache: std::collections::HashMap<u32, process::ProcessInfo> =
        std::collections::HashMap::with_capacity(32);
    for raw in window::enumerate_raw() {
        // 允许同进程窗口进入候选：匹配仍要求「可执行路径 + 窗口类名」一致，
        // 因此不会误判到 WorkScene 自己的界面；模拟窗口（win::mock）也才可被恢复测试。
        if filter::exclude_reason(&raw, self_pid, true).is_some() {
            continue;
        }
        let pinfo = pid_cache
            .entry(raw.pid)
            .or_insert_with(|| process::info(raw.pid));
        let effective = if raw.show_state == ShowState::Minimized {
            raw.restored_rect
        } else {
            raw.rect
        };
        out.push(Candidate {
            hwnd: raw.hwnd as usize,
            pid: raw.pid,
            process_path: pinfo.path.clone(),
            process_name: pinfo.name.clone(),
            class_name: raw.class_name,
            title: raw.title,
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
    let raw = from_ratios(
        (win.x_ratio, win.y_ratio, win.width_ratio, win.height_ratio),
        work,
    );

    // 显示器在：只保证"看得见"，允许贴边/略微超出（与捕获时一致）
    // 显示器不在（回退到主屏）：完全夹进工作区，避免窗口落在看不见的地方
    let area = if by_key.is_some() {
        clamp_visible(raw, work)
    } else {
        clamp(raw, work)
    };

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
        tracing::debug!(
            window = %win.title,
            process = %win.process_name,
            path = ?win.process_path,
            class = %win.class_name,
            candidates = pool.len(),
            picked = outcome.picked.is_some(),
            ambiguous = outcome.ambiguous,
            note = %outcome.note,
            "restore preview match"
        );
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

    // 资源（文件 / 文件夹 / 网址）：预览只判断可达性，不打开任何东西
    for r in &scene.resources {
        let exists = resource_exists(r);
        if exists {
            ready += 1;
        } else {
            missing += 1;
        }
        items.push(PreviewItem {
            target_name: r.display_name.clone(),
            detail: match r.item_type.as_str() {
                "url" => format!("将用默认浏览器打开 {}", r.target),
                "folder" => format!("将用资源管理器打开 {}", r.target),
                _ => format!("将用默认程序打开 {}", r.target),
            },
            status: if exists {
                StepStatus::Success
            } else {
                StepStatus::Failed
            },
            match_note: if exists {
                "资源可达".to_string()
            } else {
                "路径不存在或网址无效".to_string()
            },
        });
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
/// 串行恢复：完整恢复场景中的全部窗口与资源
pub fn apply(
    app: Option<&AppHandle>,
    scene: &SceneFile,
    relaunch_missing: bool,
    pre_restore_scene_id: Option<String>,
) -> AppResult<RestoreReport> {
    apply_with_options(
        app,
        scene,
        relaunch_missing,
        false,
        pre_restore_scene_id,
        None,
    )
}

/// 按可选目标白名单恢复（用于「仅重试失败/未完成项」，不重复折腾已成功窗口）
pub fn apply_subset(
    app: Option<&AppHandle>,
    scene: &SceneFile,
    relaunch_missing: bool,
    pre_restore_scene_id: Option<String>,
    only_targets: Option<&HashSet<String>>,
) -> AppResult<RestoreReport> {
    apply_with_options(
        app,
        scene,
        relaunch_missing,
        false,
        pre_restore_scene_id,
        only_targets,
    )
}

/// 完整恢复入口：支持专注模式（自动最小化非本空间窗口）与可选目标白名单
pub fn apply_with_options(
    app: Option<&AppHandle>,
    scene: &SceneFile,
    relaunch_missing: bool,
    focus_mode: bool,
    pre_restore_scene_id: Option<String>,
    only_targets: Option<&HashSet<String>>,
) -> AppResult<RestoreReport> {
    let started_at = now_ms();
    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Err(AppError::msg("未检测到显示器"));
    }
    let self_pid = std::process::id();

    let filtered_scene_storage: Option<SceneFile> = only_targets.map(|targets| {
        let windows = scene
            .windows
            .iter()
            .filter(|w| {
                let name = if w.title.trim().is_empty() {
                    &w.process_name
                } else {
                    &w.title
                };
                targets.contains(name)
            })
            .cloned()
            .collect();
        let resources = scene
            .resources
            .iter()
            .filter(|r| targets.contains(&r.display_name))
            .cloned()
            .collect();
        SceneFile {
            windows,
            resources,
            ..scene.clone()
        }
    });
    let scene = filtered_scene_storage.as_ref().unwrap_or(scene);

    let mut steps: Vec<RestoreStepResult> = Vec::with_capacity(scene.windows.len());
    let mut display_notes: Vec<String> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();
    let mut current = current_candidates(self_pid);
    tracing::info!(
        scene = %scene.name,
        scene_id = %scene.id,
        windows = scene.windows.len(),
        resources = scene.resources.len(),
        candidates = current.len(),
        relaunch_missing,
        focus_mode,
        restricted_targets = only_targets.is_some(),
        "restore apply start"
    );

    // 先按复用策略补齐实例：保留每个窗口独立的 launch_args / priority / delay_ms，
    // 且记录启动前已有句柄，避免在软件已打开旧实例时瞬间误判为新窗口就绪
    if relaunch_missing {
        let mut launched_total: usize = 0;
        let mut launched_per_exe: HashMap<String, usize> = HashMap::new();
        let plans = plan_launches(scene, &current);
        tracing::info!(plans = plans.len(), "restore launch plan");
        for plan in plans {
            if is_cancelled() {
                tracing::warn!("restore launch aborted by cancel request");
                break;
            }
            if launched_total >= MAX_LAUNCH_TOTAL {
                tracing::warn!(
                    cap = MAX_LAUNCH_TOTAL,
                    "restore launch total cap reached; remaining apps will be skipped"
                );
                break;
            }
            if !std::path::Path::new(&plan.exe).is_file() {
                tracing::warn!(exe = %plan.exe, "launch skipped: executable missing");
                continue;
            }
            let exe_key = plan.exe.to_lowercase();
            let exe_count = launched_per_exe.entry(exe_key).or_insert(0);
            if *exe_count >= MAX_RELAUNCH_PER_EXE {
                tracing::warn!(
                    exe = %plan.exe,
                    cap = MAX_RELAUNCH_PER_EXE,
                    "restore per-exe launch cap reached"
                );
                continue;
            }
            let known_hwnds: HashSet<usize> = current.iter().map(|c| c.hwnd).collect();
            launch_app(&plan.exe, &plan.args);
            *exe_count += 1;
            launched_total += 1;
            if wait_for_process_window(&plan.exe, &known_hwnds, self_pid) {
                current = current_candidates(self_pid);
            } else {
                tracing::warn!(
                    exe = %plan.exe,
                    args = ?plan.args,
                    "launch wait timed out; no new window detected"
                );
            }
            if plan.delay_ms > 0 && !is_cancelled() {
                let mut left = plan.delay_ms.min(15_000);
                while left > 0 && !is_cancelled() {
                    let step = left.min(100);
                    std::thread::sleep(Duration::from_millis(step));
                    left -= step;
                }
                current = current_candidates(self_pid);
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

        if is_cancelled() {
            let step = RestoreStepResult {
                step_type: "window".into(),
                target_name,
                status: StepStatus::Skipped,
                message: "已取消恢复任务（不会关闭已启动的应用）".into(),
                match_note: "用户主动取消".into(),
                duration_ms: 0,
                delta_x: None,
                delta_y: None,
                state_restored: false,
                code: Some("cancelled".into()),
            };
            if let Some(app) = app {
                let _ = app.emit("restore:step", &step);
            }
            steps.push(step);
            continue;
        }

        let target_info = resolve_target(win, &monitors);
        if let Some(n) = &target_info.note {
            if !display_notes.contains(n) {
                display_notes.push(n.clone());
            }
        }
        let rect = target_info.rect;
        // 阈值 = max(8px, 2×scale, 窗口宽度的 0.5%)：
        // 大窗口上 8px 的绝对误差没有视觉意义，按比例算更贴近"看起来一样"
        let threshold =
            delta_threshold(target_info.dpi).max(((rect.2 as f64) * 0.005).round() as i32);

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
        if relaunch_missing && !exe_exists {
            tracing::warn!(
                window = %target_name,
                process = %win.process_name,
                path = ?win.process_path,
                "window cannot be auto-launched: executable missing or unreadable"
            );
        }
        let outcome = match_window(win, &pool, rect);
        tracing::debug!(
            window = %target_name,
            process = %win.process_name,
            path = ?win.process_path,
            class = %win.class_name,
            display = %win.monitor_key,
            show_state = ?win.show_state,
            target_rect = ?rect,
            candidates = pool.len(),
            picked = outcome.picked.is_some(),
            ambiguous = outcome.ambiguous,
            note = %outcome.note,
            "restore window match"
        );
        if outcome.picked.is_none() {
            tracing::warn!(
                window = %target_name,
                process = %win.process_name,
                path = ?win.process_path,
                class = %win.class_name,
                note = %outcome.note,
                "restore window skipped"
            );
        }

        let step = if let Some(idx) = outcome.picked {
            let target = &pool[idx];
            used.insert(target.hwnd);
            let hwnd = target.hwnd as HWND;

            let mut state_restored = place_window(hwnd, rect, win.show_state);
            let elapsed = || t0.elapsed().as_millis() as i64;

            {
                // 先等 250ms 看是否直接到位（大多数应用）
                let mut delta = wait_for_settle(
                    hwnd,
                    &target_info,
                    win.show_state,
                    threshold,
                    Duration::from_millis(250),
                );
                if delta.0 > threshold
                    || (win.show_state == ShowState::Maximized && !state_restored)
                {
                    // 二次校正 + 更长的稳定窗口（慢应用 / 自己会挪窗口的应用）
                    state_restored = place_window(hwnd, rect, win.show_state);
                    delta = wait_for_settle(
                        hwnd,
                        &target_info,
                        win.show_state,
                        threshold,
                        Duration::from_millis(900),
                    );
                }

                let position_ok = win.show_state == ShowState::Minimized || delta.0 <= threshold;
                let maximized_failed = win.show_state == ShowState::Maximized && !state_restored;

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
                    "已保持最小化并恢复还原位置".into()
                } else if win.show_state == ShowState::Maximized {
                    "已最大化到目标显示器".into()
                } else {
                    format!("已恢复，位置/尺寸偏差 {}px", delta.0)
                };

                let code = if !position_ok {
                    Some("layout_drift".to_string())
                } else if maximized_failed {
                    Some("state_not_restored".to_string())
                } else {
                    None
                };

                RestoreStepResult {
                    step_type: "window".into(),
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
                step_type: "window".into(),
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
                step_type: "window".into(),
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

        tracing::info!(
            step_type = %step.step_type,
            target = %step.target_name,
            status = ?step.status,
            code = ?step.code,
            duration_ms = step.duration_ms,
            delta_x = ?step.delta_x,
            delta_y = ?step.delta_y,
            state_restored = step.state_restored,
            message = %step.message,
            "restore step finished"
        );

        // 真实进度：每完成一步推送一次（无 UI 的自检里 app 为 None）
        if let Some(app) = app {
            let _ = app.emit("restore:step", &step);
        }
        steps.push(step);
    }

    // 资源恢复：窗口摆好之后再打开文件/文件夹/网址，避免被窗口抢走焦点
    // 记住本次恢复归属于该工作空间的窗口句柄，供「一键收工」精准关闭
    if !used.is_empty() && only_targets.is_none() {
        remember_workspace_hwnds(&scene.id, used.iter().copied().collect());
    }

    // 专注模式：在打开本空间资源（网页/文件/文件夹）**之前**安静最小化无关窗口，
    // 防止刚打开的资源窗口反被误最小化
    if focus_mode && !is_cancelled() && only_targets.is_none() {
        let mut minimized = 0usize;
        for c in current_candidates(self_pid) {
            if c.pid == self_pid || used.contains(&c.hwnd) {
                continue;
            }
            let hwnd = c.hwnd as HWND;
            if !window::is_iconic(hwnd) && window::minimize_window(hwnd) {
                minimized += 1;
            }
        }
        if minimized > 0 {
            let step = RestoreStepResult {
                step_type: "focus".into(),
                target_name: "专注模式清理".into(),
                status: StepStatus::Success,
                message: format!("已自动最小化 {minimized} 个非本空间窗口（未关闭任何软件）"),
                match_note: "focus_mode".into(),
                duration_ms: 0,
                delta_x: None,
                delta_y: None,
                state_restored: true,
                code: None,
            };
            if let Some(app) = app {
                let _ = app.emit("restore:step", &step);
            }
            steps.push(step);
        }
    }

    for r in &scene.resources {
        if is_cancelled() {
            let step = RestoreStepResult {
                step_type: "resource".into(),
                target_name: r.display_name.clone(),
                status: StepStatus::Skipped,
                message: "已取消恢复任务".into(),
                match_note: r.target.clone(),
                duration_ms: 0,
                delta_x: None,
                delta_y: None,
                state_restored: false,
                code: Some("cancelled".into()),
            };
            if let Some(app) = app {
                let _ = app.emit("restore:step", &step);
            }
            steps.push(step);
            continue;
        }
        let t0 = Instant::now();
        let ok = open_resource(scene, r);
        let step = RestoreStepResult {
            step_type: "resource".into(),
            target_name: r.display_name.clone(),
            status: if ok {
                StepStatus::Success
            } else {
                StepStatus::Failed
            },
            message: if ok {
                match r.item_type.as_str() {
                    "url" => "已交给默认浏览器打开".to_string(),
                    "folder" => "已在资源管理器中打开".to_string(),
                    _ => "已用默认程序打开".to_string(),
                }
            } else {
                format!("无法打开：{}（路径不存在或没有关联程序）", r.target)
            },
            match_note: r.target.clone(),
            duration_ms: t0.elapsed().as_millis() as i64,
            delta_x: None,
            delta_y: None,
            state_restored: true,
            code: if ok {
                None
            } else {
                Some("resource_open_failed".to_string())
            },
        };

        if let Some(app) = app {
            let _ = app.emit("restore:step", &step);
        }
        steps.push(step);
    }

    let succeeded = steps
        .iter()
        .filter(|s| s.status == StepStatus::Success)
        .count();
    let failed = steps
        .iter()
        .filter(|s| s.status == StepStatus::Failed)
        .count();
    let skipped = steps
        .iter()
        .filter(|s| matches!(s.status, StepStatus::Skipped | StepStatus::NeedsReview))
        .count();

    tracing::info!(
        run_id = %format!("run-{:x}", started_at),
        scene = %scene.name,
        succeeded,
        failed,
        skipped,
        total = steps.len(),
        "restore apply finished"
    );
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

/// 复用策略三值（PRD FR-005/006 的交界，此前只有默认行为）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReusePolicy {
    /// 只复用已存在且能被匹配的窗口，从不启动新实例
    ReuseIfMatched,
    /// 每个快照窗口都新启一个实例
    AlwaysLaunch,
    /// 缺几个补几个（默认）
    ReuseOrLaunch,
}

pub fn parse_policy(raw: &str) -> ReusePolicy {
    match raw.trim().to_lowercase().as_str() {
        "reuse_if_matched" => ReusePolicy::ReuseIfMatched,
        "always_launch" => ReusePolicy::AlwaysLaunch,
        _ => ReusePolicy::ReuseOrLaunch,
    }
}

/// 按类 shell 规则切分参数：支持单/双引号包住空格，不做转义与变量展开。
/// 参数始终以数组形式交给 CreateProcess，绝不拼接成 cmd.exe 命令行（文档 §14.2）。
pub fn split_args(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_token = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// 一键收工：优先关闭本次恢复记录的本空间窗口句柄，再对剩余窗口做严格标题/位置匹配后发送 WM_CLOSE
/// （绝不强杀进程，也不误关标题和位置都无关的其他软件窗口）
pub fn close_workspace_windows(scene: &SceneFile) -> AppResult<usize> {
    let monitors = display::enumerate();
    if monitors.is_empty() {
        return Ok(0);
    }
    let self_pid = std::process::id();
    let current = current_candidates(self_pid);
    let mut used: HashSet<usize> = HashSet::new();
    let mut closed = 0usize;

    // 1. 优先关闭本会话恢复时精确记录的窗口句柄
    let tracked = take_workspace_hwnds(&scene.id);
    for hwnd in tracked {
        if current.iter().any(|c| c.hwnd == hwnd && c.pid != self_pid) && used.insert(hwnd) {
            if window::close_window_gracefully(hwnd as HWND) {
                closed += 1;
            }
        }
    }

    // 2. 若未通过本会话恢复（或仍有未关闭项），按严格匹配（需标题相似或位置重合）补充关闭
    if closed < scene.windows.len() {
        for win in &scene.windows {
            let target = resolve_target(win, &monitors);
            let pool: Vec<Candidate> = current
                .iter()
                .filter(|c| c.pid != self_pid && !used.contains(&c.hwnd))
                .cloned()
                .collect();
            if let Some(idx) = match_window_for_close(win, &pool, target.rect) {
                let hwnd = pool[idx].hwnd;
                used.insert(hwnd);
                if window::close_window_gracefully(hwnd as HWND) {
                    closed += 1;
                }
            }
        }
    }
    Ok(closed)
}

struct LaunchPlan {
    exe: String,
    args: Vec<String>,
    priority: i64,
    delay_ms: u64,
}

/// 当桌面已有同进程实例时，为常见单实例转发型应用注入「新开窗口」参数，避免新启动只切焦点不开窗
fn inject_new_window_flag(exe: &str, args: &mut Vec<String>) {
    let name = exe
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let has_flag = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));
    match name.as_str() {
        "firefox.exe" => {
            // Firefox 的命令行选项是单横线 `-new-window`，不是 Chromium 的 `--new-window`。
            if !has_flag("-new-window") && !has_flag("--new-window") {
                args.insert(0, "-new-window".into());
            }
        }
        "chrome.exe" | "msedge.exe" | "brave.exe" | "vivaldi.exe" | "arc.exe" => {
            if !has_flag("--new-window") && !has_flag("-new-window") {
                args.insert(0, "--new-window".into());
            }
        }
        "code.exe"
        | "code - insiders.exe"
        | "cursor.exe"
        | "windsurf.exe"
        | "trae.exe"
        | "antigravity.exe" => {
            if !has_flag("-n") && !has_flag("--new-window") {
                args.insert(0, "-n".into());
            }
        }
        "wt.exe" => {
            if !has_flag("-w") && !has_flag("--window") {
                args.insert(0, "new".into());
                args.insert(0, "-w".into());
            }
        }
        _ => {}
    }
}

/// 按窗口分别规划补启任务：保留每个窗口独立的 launch_args / priority / delay_ms，
/// 并根据当前已有实例数决定需要补启几个（ReuseOrLaunch / AlwaysLaunch / ReuseIfMatched）
fn plan_launches(scene: &SceneFile, current: &[Candidate]) -> Vec<LaunchPlan> {
    let mut available_by_exe: HashMap<String, usize> = HashMap::new();
    for c in current {
        if let Some(p) = &c.process_path {
            *available_by_exe.entry(p.to_lowercase()).or_insert(0) += 1;
        }
    }

    let mut windows_ordered: Vec<&SceneWindow> = scene.windows.iter().collect();
    windows_ordered.sort_by_key(|w| w.launch_priority);

    let mut plans: Vec<LaunchPlan> = Vec::new();
    let mut running_count: HashMap<String, usize> = available_by_exe.clone();

    for w in windows_ordered {
        let Some(path) = w.process_path.as_ref() else {
            continue;
        };
        let key = path.to_lowercase();
        let should_launch = match parse_policy(&w.reuse_policy) {
            ReusePolicy::AlwaysLaunch => true,
            ReusePolicy::ReuseIfMatched => false,
            ReusePolicy::ReuseOrLaunch => {
                let avail = available_by_exe.entry(key.clone()).or_insert(0);
                if *avail > 0 {
                    *avail -= 1;
                    false
                } else {
                    true
                }
            }
        };

        if !should_launch {
            continue;
        }

        let mut args = split_args(w.launch_args.as_deref().unwrap_or(""));
        let existing = running_count.entry(key).or_insert(0);
        if *existing > 0 {
            inject_new_window_flag(path, &mut args);
        }
        *existing += 1;

        plans.push(LaunchPlan {
            exe: path.clone(),
            args,
            priority: w.launch_priority,
            delay_ms: w.launch_delay_ms,
        });
    }

    plans.sort_by_key(|p| p.priority);
    plans
}

fn delta_threshold(dpi: u32) -> i32 {
    let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
    BASE_DELTA_PX.max((2.0 * scale).round() as i32)
}

/// 恢复窗口位置与显示状态。返回「显示状态是否与快照一致」。
///
/// 注意：不以 SetWindowPlacement 的返回值判成败。实测发现 Chromium 系窗口
/// 会返回 FALSE，但位置其实已经改对了（SetWindowPos 生效）——用返回值判会把
/// 成功的恢复报成失败。这里只负责尝试，成败交给后续的实测校验。
fn place_window(hwnd: HWND, rect: (i32, i32, i32, i32), target_state: ShowState) -> bool {
    if !window::is_window(hwnd) {
        return false;
    }

    match target_state {
        ShowState::Maximized => {
            // 先给还原矩形，再最大化；SetWindowPlacement 失败时用 ShowWindow(SW_SHOWMAXIMIZED) 兜底
            window::set_window_rect(hwnd, rect.0, rect.1, rect.2, rect.3);
            let _ =
                window::apply_placement(hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Maximized);
            if !window::is_zoomed(hwnd) {
                let _ = window::show_maximized(hwnd);
            }
        }
        ShowState::Minimized => {
            // 保持后台最小化，同时写好还原矩形，不强行弹满屏幕
            let _ =
                window::apply_placement(hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Minimized);
            if !window::is_iconic(hwnd) {
                let _ = window::minimize_window(hwnd);
            }
        }
        ShowState::Normal => {
            let placed =
                window::apply_placement(hwnd, rect.0, rect.1, rect.2, rect.3, ShowState::Normal);
            if !placed {
                // 退一步：只改位置，至少布局对得上
                window::set_window_rect(hwnd, rect.0, rect.1, rect.2, rect.3);
            }
        }
    }

    match target_state {
        ShowState::Maximized => window::is_zoomed(hwnd),
        ShowState::Normal => !window::is_zoomed(hwnd) && !window::is_iconic(hwnd),
        ShowState::Minimized => window::is_iconic(hwnd),
    }
}

/// 等窗口稳定后再判定：有些应用（资源管理器、部分 Electron）会在我们设置位置之后
/// 异步把自己再挪一次。固定 sleep 160ms 采样会抓到"中间态"，把最终正确的恢复判成失败。
fn wait_for_settle(
    hwnd: HWND,
    target: &Target,
    state: ShowState,
    threshold: i32,
    budget: Duration,
) -> (i32, i32, i32) {
    let end = Instant::now() + budget;
    let mut last = verify(hwnd, target, state);
    let mut hits = 0;

    while Instant::now() < end {
        if last.0 <= threshold {
            // 连续两次都达标才算稳定：Chromium 移动窗口有动画，
            // 单次采样可能正好落在动画中途
            hits += 1;
            if hits >= 2 {
                break;
            }
        } else {
            hits = 0;
        }
        std::thread::sleep(Duration::from_millis(120));
        last = verify(hwnd, target, state);
    }
    last
}

/// 四边偏差：返回 (最大偏差, dx, dy)，尺寸偏差计入最大值（评审补充意见 D4）。
///
/// 最大化窗口的 GetWindowRect 返回的是整块显示器（含不可见边框），
/// 不能拿它和还原矩形比较，否则会把成功的最大化判成失败；
/// 这种情况下只校验「窗口中心是否落在目标显示器内」+ 是否真的处于最大化。
fn verify(hwnd: HWND, target: &Target, state: ShowState) -> (i32, i32, i32) {
    // 最小化窗口的 GetWindowRect 是 (-32000,-32000)，拿它和目标还原矩形比较
    // 会得到 3 万像素的假偏差，还会让 wait_for_settle 误判需要二次校正。
    // 最小化状态本身由 place_window/state_restored 校验，这里不再比较位置。
    if state == ShowState::Minimized {
        return (if window::is_iconic(hwnd) { 0 } else { i32::MAX }, 0, 0);
    }

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

/// 资源统一由系统默认程序打开（文件夹走 Windows 资源管理器，网址走默认浏览器，文件走默认关联程序），
/// 避免把普通文档/素材文件夹强行塞给 IDE 打开。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceOpener {
    Default,
}

#[allow(dead_code)]
pub fn choose_opener(_scene: &SceneFile, _r: &SceneResource) -> ResourceOpener {
    ResourceOpener::Default
}

/// 资源是否可达：网址看协议，文件/文件夹看存在性
fn resource_exists(r: &SceneResource) -> bool {
    match r.item_type.as_str() {
        "url" => r.target.starts_with("http://") || r.target.starts_with("https://"),
        _ => std::path::Path::new(&r.target).exists(),
    }
}

/// 用系统默认关联打开资源（ShellExecuteW，"open" 动词）。
/// 不走 cmd.exe，也不拼接命令行字符串。
fn open_resource(_scene: &SceneFile, r: &SceneResource) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    const SW_SHOWNORMAL: i32 = 1;

    if !resource_exists(r) {
        return false;
    }

    let verb: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = r.target.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let ret = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        (ret as isize) > 32
    }
}

fn launch_app(exe: &str, args: &[String]) {
    let mut cmd = std::process::Command::new(exe);
    if !args.is_empty() {
        cmd.args(args);
    }
    if let Some(dir) = std::path::Path::new(exe).parent() {
        cmd.current_dir(dir);
    }
    // 参数以数组传入，不拼接 Shell 字符串（文档 §14.2）
    let _ = cmd.spawn();
}

/// 等待目标 exe 真正创建出**新的**有效可见窗口（排除启动前已存在的 known_hwnds 及系统/闪屏噪声窗）
fn wait_for_process_window(exe: &str, known_hwnds: &HashSet<usize>, self_pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_millis(LAUNCH_WAIT_MS);
    while Instant::now() < deadline && !is_cancelled() {
        std::thread::sleep(Duration::from_millis(120));
        let mut pid_cache: HashMap<u32, Option<String>> = HashMap::with_capacity(16);
        let (raws, _) = window::enumerate_visible(Some(self_pid));
        for raw in raws {
            let hwnd = raw.hwnd as usize;
            if known_hwnds.contains(&hwnd) {
                continue;
            }
            if filter::exclude_reason(&raw, self_pid, false).is_some() {
                continue;
            }
            let path = pid_cache
                .entry(raw.pid)
                .or_insert_with(|| process::info(raw.pid).path);
            if let Some(p) = path {
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
    fn opener_always_uses_default_system_association() {
        let scene = scene_with(
            "reuse_or_launch",
            r"C:\Program Files\Microsoft VS Code\Code.exe",
            1,
        );
        let folder = SceneResource {
            item_type: "folder".into(),
            target: r"D:\work\app".into(),
            display_name: "app".into(),
        };
        assert_eq!(choose_opener(&scene, &folder), ResourceOpener::Default);

        let file = SceneResource {
            item_type: "file".into(),
            target: r"D:\work\app\README.md".into(),
            display_name: "README".into(),
        };
        assert_eq!(choose_opener(&scene, &file), ResourceOpener::Default);
    }

    #[test]
    fn opener_falls_back_to_default_without_ide() {
        let scene = scene_with("reuse_or_launch", r"C:\app\notepad.exe", 1);
        let folder = SceneResource {
            item_type: "folder".into(),
            target: r"D:\work".into(),
            display_name: "work".into(),
        };
        assert_eq!(choose_opener(&scene, &folder), ResourceOpener::Default);
    }

    /// 收工（批量关闭）依赖 matcher 的判定：这里钉住"不同应用绝不会被认成同一窗口"，
    /// 否则用户点一次收工可能关掉别的软件。真实窗口枚举没法在单测里复现，
    /// 所以测的是决策点本身（match_window 返回 None 时收工不会动手）。
    #[test]
    fn matcher_never_picks_unrelated_window() {
        let scene = scene_with(
            "reuse_or_launch",
            r"C:\Program Files\Mozilla Firefox\firefox.exe",
            1,
        );
        let target = (0, 0, 1920, 1080);
        let intruder = Candidate {
            hwnd: 999,
            pid: 4242,
            process_path: Some(r"C:\Other\explorer.exe".into()),
            process_name: "explorer.exe".into(),
            class_name: "CabinetWClass".into(),
            title: "完全无关的窗口".into(),
            rect: (0, 0, 800, 600),
        };

        let outcome = match_window(&scene.windows[0], &[intruder], target);
        assert!(
            outcome.picked.is_none(),
            "不相关窗口被匹配上了，收工会误关：{}",
            outcome.note
        );

        // 同名同类的候选才允许被选中，确认判定不是"永远返回 None"
        let same = Candidate {
            hwnd: 1000,
            pid: 4243,
            process_path: Some(r"C:\Program Files\Mozilla Firefox\firefox.exe".into()),
            process_name: "firefox.exe".into(),
            class_name: "Class".into(),
            title: "w0".into(),
            rect: (0, 0, 800, 600),
        };
        let hit = match_window(&scene.windows[0], &[same], target);
        assert!(hit.picked.is_some(), "同一应用应能匹配：{}", hit.note);
    }
    #[test]
    fn threshold_never_drops_below_base() {
        assert_eq!(delta_threshold(96), 8);
        assert_eq!(delta_threshold(144), 8);
        assert_eq!(delta_threshold(480), 10);
    }

    #[test]
    fn policy_parses_three_values_and_defaults_safely() {
        assert_eq!(
            parse_policy("reuse_if_matched"),
            ReusePolicy::ReuseIfMatched
        );
        assert_eq!(parse_policy("ALWAYS_LAUNCH"), ReusePolicy::AlwaysLaunch);
        assert_eq!(parse_policy("reuse_or_launch"), ReusePolicy::ReuseOrLaunch);
        // 未知值必须退化成最保守的默认策略，而不是 panic 或不启动
        assert_eq!(parse_policy("garbage"), ReusePolicy::ReuseOrLaunch);
    }

    #[test]
    fn args_split_supports_quotes_and_spaces() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("--new-window"), vec!["--new-window"]);
        assert_eq!(
            split_args(r#"--profile "C:\Work Space" --flag"#),
            vec!["--profile", r"C:\Work Space", "--flag"]
        );
        assert_eq!(split_args("'a b' c"), vec!["a b", "c"]);
        assert_eq!(split_args("  a   b  "), vec!["a", "b"]);
    }

    fn scene_with(policy: &str, path: &str, count: usize) -> SceneFile {
        let windows = (0..count)
            .map(|i| SceneWindow {
                process_path: Some(path.into()),
                process_name: "app.exe".into(),
                class_name: "Class".into(),
                title: format!("w{i}"),
                monitor_key: "M1".into(),
                x_ratio: 0.0,
                y_ratio: 0.0,
                width_ratio: 0.5,
                height_ratio: 0.5,
                original_x: 0,
                original_y: 0,
                original_width: 800,
                original_height: 600,
                source_dpi: 96,
                show_state: ShowState::Normal,
                z_order: i as i32,
                reuse_policy: policy.into(),
                launch_args: None,
                launch_priority: 0,
                launch_delay_ms: 0,
            })
            .collect();
        SceneFile {
            schema_version: 1,
            id: "s".into(),
            name: "s".into(),
            captured_at: 0,
            displays: vec![],
            windows,
            resources: Vec::new(),
        }
    }

    fn candidate(path: &str, n: usize) -> Vec<Candidate> {
        (0..n)
            .map(|_| Candidate {
                hwnd: 1,
                pid: 1,
                process_path: Some(path.into()),
                process_name: "app.exe".into(),
                class_name: "Class".into(),
                title: "w".into(),
                rect: (0, 0, 100, 100),
            })
            .collect()
    }

    #[test]
    fn reuse_or_launch_only_fills_the_gap() {
        let scene = scene_with("reuse_or_launch", r"C:\app\a.exe", 3);
        let plans = plan_launches(&scene, &candidate(r"C:\app\a.exe", 1));
        assert_eq!(plans.len(), 2, "3 个窗口、已开 1 个 -> 补 2 个独立启动计划");
    }

    #[test]
    fn reuse_if_matched_never_launches() {
        let scene = scene_with("reuse_if_matched", r"C:\app\a.exe", 3);
        assert!(plan_launches(&scene, &candidate(r"C:\app\a.exe", 1)).is_empty());
        assert!(plan_launches(&scene, &[]).is_empty());
    }

    #[test]
    fn always_launch_ignores_existing_windows() {
        let scene = scene_with("always_launch", r"C:\app\a.exe", 2);
        let plans = plan_launches(&scene, &candidate(r"C:\app\a.exe", 5));
        assert_eq!(plans.len(), 2, "always_launch 不管已开多少都新启");
    }

    #[test]
    fn firefox_uses_single_dash_new_window_flag() {
        let scene = scene_with(
            "reuse_or_launch",
            r"C:\Program Files\Mozilla Firefox\firefox.exe",
            2,
        );
        let plans = plan_launches(
            &scene,
            &candidate(r"C:\Program Files\Mozilla Firefox\firefox.exe", 1),
        );
        assert_eq!(plans.len(), 1);
        assert!(plans[0].args.iter().any(|a| a == "-new-window"));
        assert!(!plans[0].args.iter().any(|a| a == "--new-window"));
    }

    #[test]
    fn multi_window_chrome_injects_new_window_flag() {
        let scene = scene_with(
            "reuse_or_launch",
            r"C:\Program Files\Google\Chrome\chrome.exe",
            2,
        );
        let plans = plan_launches(
            &scene,
            &candidate(r"C:\Program Files\Google\Chrome\chrome.exe", 1),
        );
        assert_eq!(plans.len(), 1);
        assert!(plans[0].args.iter().any(|a| a == "--new-window"));
    }
}
