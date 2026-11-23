use crate::core::models::{CaptureResult, SceneFile, SceneSummary, SceneWindow, ShowState};
use crate::core::{AppError, AppResult};
use crate::restore::layout::{normalize_ratios, to_ratios, Area};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 同一毫秒内可能创建多份快照（并行测试、快速连续捕获），
/// 用进程内序号保证 id 唯一，避免两个写入抢同一个 .tmp 文件。
static SCENE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 数据根目录。
///
/// 解析顺序（第一个真正可写的胜出）：
/// 1. `WORKSCENE_DATA_DIR`（显式指定，不再回退——便携模式与测试都靠它）
/// 2. `%LOCALAPPDATA%\WorkScene`
/// 3. `%APPDATA%\WorkScene`
/// 4. exe 同级的 `data\`
/// 5. `%TEMP%\WorkScene`
///
/// 为什么要回退：默认目录在某些账户/受管环境下不可写，早期版本会直接
/// "静默退出"（release 无控制台，用户看到的就是点了没反应）。
pub fn data_dir() -> PathBuf {
    DATA_DIR.get_or_init(resolve_data_dir).clone()
}

fn resolve_data_dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("WORKSCENE_DATA_DIR") {
        let p = PathBuf::from(custom);
        if !p.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
    }

    for candidate in fallback_candidates() {
        if is_writable(&candidate) {
            return candidate;
        }
    }

    // 全都不行时仍然返回默认路径，让上层把错误如实报出来
    local_app_data().join("WorkScene")
}

fn fallback_candidates() -> Vec<PathBuf> {
    let mut out = vec![local_app_data().join("WorkScene")];

    if let Some(appdata) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(appdata).join("WorkScene"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("data"));
        }
    }
    out.push(std::env::temp_dir().join("WorkScene"));
    out
}

/// 真正探测一次写入：只看目录是否存在不足以判断（ACL 可能允许读不允许写）
fn is_writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"probe") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
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

    let mut id = format!(
        "scene-{:x}-{:x}",
        capture.captured_at,
        SCENE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let mut n = 1;
    while scene_path(&id).exists() {
        id = format!(
            "scene-{:x}-{}-{n}",
            capture.captured_at,
            SCENE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
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
            Some(d) => normalize_ratios(to_ratios(
                Area::new(w.x as f64, w.y as f64, w.width as f64, w.height as f64),
                Area::new(
                    d.work_x as f64,
                    d.work_y as f64,
                    d.work_width as f64,
                    d.work_height as f64,
                ),
            )),
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
            reuse_policy: "reuse_or_launch".into(),
            launch_args: None,
            launch_priority: 0,
            launch_delay_ms: 0,
        });
    }

    let scene = SceneFile {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: name.clone(),
        captured_at: capture.captured_at,
        displays: capture.displays.clone(),
        windows,
        resources: Vec::new(),
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

/// 将已构造的 SceneFile 落盘为不可变快照（用于覆盖工作空间前的自动保护）
pub fn save_scene_file(name: &str, mut scene: SceneFile) -> AppResult<SceneSummary> {
    ensure_dirs()?;
    let now = crate::core::time::now_ms();
    let trimmed = name.trim();
    let label = if trimmed.is_empty() {
        format!("保护快照 {}", short_time(now))
    } else {
        trimmed.to_string()
    };

    let mut id = format!(
        "scene-{:x}-{:x}",
        now,
        SCENE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let mut n = 1;
    while scene_path(&id).exists() {
        id = format!(
            "scene-{:x}-{}-{n}",
            now,
            SCENE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        n += 1;
    }

    scene.schema_version = SCHEMA_VERSION;
    scene.id = id.clone();
    scene.name = label.clone();
    scene.captured_at = now;

    let json = serde_json::to_string_pretty(&scene)?;
    write_atomic(&scene_path(&id), &json)?;

    Ok(SceneSummary {
        id: id.clone(),
        name: label,
        captured_at: now,
        window_count: scene.windows.len(),
        display_count: scene.displays.len(),
        path: scene_path(&id).to_string_lossy().to_string(),
    })
}

/// 覆盖已有快照内容（仅用于快照富化：资源/启动参数回填）。
/// 必须走与其他快照相同的原子写，否则断电会留下半个 JSON。
pub fn overwrite_scene(scene: &SceneFile) -> AppResult<()> {
    if !valid_id(&scene.id) {
        return Err(AppError::msg("非法的快照 ID"));
    }
    ensure_dirs()?;
    let json = serde_json::to_string_pretty(scene)?;
    write_atomic(&scene_path(&scene.id), &json)
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
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}.bak", path.display()));
    let _ = std::fs::remove_file(format!("{}.tmp", path.display()));
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
