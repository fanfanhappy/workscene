use crate::core::models::{
    CaptureResult, RestoreReport, SceneFile, SceneSummary, SceneWindow, ShowState,
};
use crate::core::{AppError, AppResult};
use crate::restore::layout::{to_ratios, Area};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

/// 数据根目录。默认 %LOCALAPPDATA%\WorkScene；
/// 可用 WORKSCENE_DATA_DIR 覆盖（便携模式、自动化测试、以及 UI 规范 §12.1 的「存储位置」设置）。
pub fn data_dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("WORKSCENE_DATA_DIR") {
        let p = PathBuf::from(custom);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("WorkScene")
}

pub fn snapshot_dir() -> PathBuf {
    data_dir().join("snapshots")
}

pub fn ensure_dirs() -> AppResult<()> {
    std::fs::create_dir_all(snapshot_dir())?;
    Ok(())
}

/// 只允许内部 id 作为文件名，杜绝路径穿越（导入数据不可信）
fn valid_id(id: &str) -> bool {
    let ok_prefix = id.starts_with("scene-");
    let ok_chars = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    ok_prefix && ok_chars && id.len() < 64
}

fn scene_path(id: &str) -> PathBuf {
    snapshot_dir().join(format!("{id}.json"))
}

pub fn save_scene(name: &str, capture: &CaptureResult) -> AppResult<SceneSummary> {
    ensure_dirs()?;

    let trimmed = name.trim();
    let name = if trimmed.is_empty() {
        format!("工作现场 {}", short_time(capture.captured_at))
    } else {
        trimmed.to_string()
    };

    let mut id = format!("scene-{:x}", capture.captured_at);
    let mut n = 1;
    while scene_path(&id).exists() {
        id = format!("scene-{:x}-{n}", capture.captured_at);
        n += 1;
    }

    let mut windows: Vec<SceneWindow> = Vec::with_capacity(capture.windows.len());
    for w in &capture.windows {
        let monitor_key = w.monitor_key.clone().unwrap_or_default();
        let work = capture
            .displays
            .iter()
            .find(|d| d.monitor_key == monitor_key)
            .or_else(|| capture.displays.iter().find(|d| d.is_primary))
            .or_else(|| capture.displays.first());

        let (x_ratio, y_ratio, width_ratio, height_ratio) = match work {
            Some(d) => to_ratios(
                Area::new(w.x as f64, w.y as f64, w.width as f64, w.height as f64),
                Area::new(
                    d.work_x as f64,
                    d.work_y as f64,
                    d.work_width as f64,
                    d.work_height as f64,
                ),
            ),
            None => (0.0, 0.0, 1.0, 1.0),
        };

        windows.push(SceneWindow {
            process_path: w.process_path.clone(),
            process_name: w.process_name.clone(),
            class_name: w.class_name.clone(),
            title: w.title.clone(),
            monitor_key,
            x_ratio,
            y_ratio,
            width_ratio,
            height_ratio,
            original_x: w.x,
            original_y: w.y,
            original_width: w.width,
            original_height: w.height,
            source_dpi: w.dpi,
            show_state: if w.show_state == ShowState::Normal {
                ShowState::Normal
            } else {
                w.show_state
            },
            z_order: windows.len() as i32,
        });
    }

    let scene = SceneFile {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: name.clone(),
        captured_at: capture.captured_at,
        displays: capture.displays.clone(),
        windows,
    };

    let json = serde_json::to_string_pretty(&scene)?;
    write_atomic(&scene_path(&id), &json)?;

    Ok(SceneSummary {
        id,
        name,
        captured_at: scene.captured_at,
        window_count: scene.windows.len(),
        display_count: scene.displays.len(),
        path: scene_path(&scene.id).to_string_lossy().to_string(),
    })
}

pub fn list_scenes() -> AppResult<Vec<SceneSummary>> {
    ensure_dirs()?;
    let mut out = Vec::new();

    for entry in std::fs::read_dir(snapshot_dir())? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(scene) = serde_json::from_str::<SceneFile>(&text) else {
            continue; // 损坏文件不阻塞列表
        };
        out.push(SceneSummary {
            id: scene.id,
            name: scene.name,
            captured_at: scene.captured_at,
            window_count: scene.windows.len(),
            display_count: scene.displays.len(),
            path: path.to_string_lossy().to_string(),
        });
    }

    out.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
    Ok(out)
}

pub fn load_scene(id: &str) -> AppResult<SceneFile> {
    if !valid_id(id) {
        return Err(AppError::msg("非法的快照 ID"));
    }
    let path = scene_path(id);
    if !path.exists() {
        return Err(AppError::SceneNotFound(id.to_string()));
    }
    let text = std::fs::read_to_string(&path)?;
    let scene: SceneFile = serde_json::from_str(&text)?;
    if scene.schema_version > SCHEMA_VERSION {
        return Err(AppError::msg(format!(
            "该快照由更新版本的 WorkScene 创建（schema {}），请升级后再打开",
            scene.schema_version
        )));
    }
    Ok(scene)
}

pub fn delete_scene(id: &str) -> AppResult<()> {
    if !valid_id(id) {
        return Err(AppError::msg("非法的快照 ID"));
    }
    let path = scene_path(id);
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path)?;
    Ok(())
}

/// 原子写入：先写 .tmp，再保留 .bak，最后 rename 覆盖。
/// 直接 fs::write 在断电/崩溃时会留下半个 JSON（评审补充意见 A3）。
fn write_atomic(path: &Path, content: &str) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;

    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = std::fs::copy(path, bak);
    }

    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorruptedScene {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneHealth {
    pub corrupted: Vec<CorruptedScene>,
}

/// 损坏文件不能静默消失：列表照常返回好数据，同时把坏文件报给 UI
pub fn scene_health() -> AppResult<SceneHealth> {
    ensure_dirs()?;
    let mut corrupted = Vec::new();

    for entry in std::fs::read_dir(snapshot_dir())? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if let Err(e) = serde_json::from_str::<SceneFile>(&text) {
                    corrupted.push(CorruptedScene {
                        file,
                        reason: format!("JSON 解析失败：{e}"),
                    });
                }
            }
            Err(e) => corrupted.push(CorruptedScene {
                file,
                reason: format!("无法读取：{e}"),
            }),
        }
    }

    Ok(SceneHealth { corrupted })
}

pub fn runs_dir() -> PathBuf {
    data_dir().join("runs")
}

/// 恢复报告落盘：关窗、刷新、崩溃后仍可回查（评审补充意见 D5）
pub fn save_run(report: &RestoreReport) -> AppResult<()> {
    std::fs::create_dir_all(runs_dir())?;
    let path = runs_dir().join(format!("{}.json", report.run_id));
    let json = serde_json::to_string_pretty(report)?;
    write_atomic(&path, &json)?;
    Ok(())
}

pub fn list_runs(limit: usize) -> AppResult<Vec<RestoreReport>> {
    let dir = runs_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(report) = serde_json::from_str::<RestoreReport>(&text) {
            out.push(report);
        }
    }
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    out.truncate(limit);
    Ok(out)
}

fn short_time(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // 简化处理：只用于默认命名，不做时区换算
    format!("D{}+{:02}:{:02}", days, rem / 3600, (rem % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::valid_id;

    #[test]
    fn rejects_path_traversal_ids() {
        assert!(!valid_id("../../evil"));
        assert!(!valid_id("scene-..\\..\\x"));
        assert!(!valid_id("SCENE-1"));
        assert!(valid_id("scene-1900000000"));
        assert!(valid_id("scene-1900000000-2"));
    }
}
