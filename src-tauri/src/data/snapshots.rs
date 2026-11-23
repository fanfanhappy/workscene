use crate::core::models::{CaptureResult, SceneFile};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::data::workspaces;
use crate::data::Db;
use crate::restore::matcher::normalize_title;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// 自动快照的默认节流参数（后续接 app_settings）
pub const AUTO_MIN_INTERVAL_MS: i64 = 180_000;
pub const AUTO_KEEP_PER_WORKSPACE: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub snapshot_type: String,
    pub trigger_type: String,
    pub captured_at: i64,
    pub window_count: i64,
    pub display_count: i64,
    pub is_protected: bool,
    pub fingerprint: Option<String>,
}

fn row_to_summary(r: &sqlx::sqlite::SqliteRow) -> SnapshotSummary {
    SnapshotSummary {
        id: r.get("id"),
        workspace_id: r.get("workspace_id"),
        name: r.get("name"),
        snapshot_type: r.get("snapshot_type"),
        trigger_type: r.get("trigger_type"),
        captured_at: r.get("captured_at"),
        window_count: r.get("window_count"),
        display_count: r.get("display_count"),
        is_protected: r.get::<i64, _>("is_protected") != 0,
        fingerprint: r.get("content_fingerprint"),
    }
}

/// 手动快照：永远创建（用户点了一次就是一次）
pub async fn create_manual(
    db: &Db,
    workspace_id: &str,
    name: &str,
    capture: &CaptureResult,
) -> AppResult<SnapshotSummary> {
    create(db, workspace_id, name, "manual", "manual", capture, true).await
}

/// 自动快照：指纹相同 → 不创建；距上次不足节流窗口 → 不创建。
/// 返回 None 表示本次跳过（这正是"自动快照不能每次窗口变化都写库"的红线二）。
pub async fn create_auto(
    db: &Db,
    workspace_id: &str,
    capture: &CaptureResult,
    min_interval_ms: i64,
) -> AppResult<Option<SnapshotSummary>> {
    let fp = workspaces::fingerprint(capture);
    // 只和上一份“自动快照”比较指纹/节流；手动或保护快照不应阻塞自动快照。
    let latest: Option<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, captured_at, content_fingerprint FROM snapshots
         WHERE workspace_id = ? AND snapshot_type = 'auto'
         ORDER BY captured_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?;

    if let Some((_id, at, Some(prev))) = &latest {
        if prev == &fp {
            return Ok(None); // 环境没变
        }
        if now_ms() - at < min_interval_ms {
            return Ok(None); // 变化太频繁，节流
        }
    }

    let summary = create(db, workspace_id, "自动快照", "auto", "auto", capture, false).await?;
    Ok(Some(summary))
}

pub async fn create(
    db: &Db,
    workspace_id: &str,
    name: &str,
    snapshot_type: &str,
    trigger: &str,
    capture: &CaptureResult,
    protected: bool,
) -> AppResult<SnapshotSummary> {
    let scene = workspaces::save_enriched_snapshot(db, workspace_id, name, capture).await?;
    let fp = workspaces::fingerprint(capture);

    sqlx::query(
        "INSERT INTO snapshots (id, workspace_id, name, snapshot_type, trigger_type,
            captured_at, window_count, display_count, payload_path, content_fingerprint,
            payload_version, is_protected)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&scene.id)
    .bind(workspace_id)
    .bind(name)
    .bind(snapshot_type)
    .bind(trigger)
    .bind(capture.captured_at)
    .bind(capture.windows.len() as i64)
    .bind(capture.displays.len() as i64)
    .bind(&scene.path)
    .bind(&fp)
    .bind(if protected { 1 } else { 0 })
    .execute(&db.pool)
    .await?;

    Ok(SnapshotSummary {
        id: scene.id,
        workspace_id: workspace_id.to_string(),
        name: name.to_string(),
        snapshot_type: snapshot_type.to_string(),
        trigger_type: trigger.to_string(),
        captured_at: capture.captured_at,
        window_count: capture.windows.len() as i64,
        display_count: capture.displays.len() as i64,
        is_protected: protected,
        fingerprint: Some(fp),
    })
}

pub async fn list(db: &Db, workspace_id: &str, limit: usize) -> AppResult<Vec<SnapshotSummary>> {
    let rows = sqlx::query(
        "SELECT id, workspace_id, name, snapshot_type, trigger_type, captured_at,
                window_count, display_count, is_protected, content_fingerprint
         FROM snapshots WHERE workspace_id = ? ORDER BY captured_at DESC LIMIT ?",
    )
    .bind(workspace_id)
    .bind(limit as i64)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.iter().map(row_to_summary).collect())
}

/// 崩溃恢复优先使用“真正的现场快照”：跳过恢复前保护快照。
/// 保护快照记录的是上一次恢复之前的状态，拿它做“恢复上次现场”会把用户带回更早的桌面。
pub async fn latest_for_recovery(
    db: &Db,
    workspace_id: &str,
) -> AppResult<Option<SnapshotSummary>> {
    let row = sqlx::query(
        "SELECT id, workspace_id, name, snapshot_type, trigger_type, captured_at,
                window_count, display_count, is_protected, content_fingerprint
         FROM snapshots
         WHERE workspace_id = ? AND snapshot_type <> 'protection'
         ORDER BY captured_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.as_ref().map(row_to_summary))
}

pub async fn get(db: &Db, id: &str) -> AppResult<SnapshotSummary> {
    let row = sqlx::query(
        "SELECT id, workspace_id, name, snapshot_type, trigger_type, captured_at,
                window_count, display_count, is_protected, content_fingerprint
         FROM snapshots WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::msg(format!("快照不存在：{id}")))?;
    Ok(row_to_summary(&row))
}

/// 删除快照 payload 及其 .bak/.tmp；文件不存在视为已删除，权限错误必须上报。
fn remove_payload_files(path: &str) -> AppResult<()> {
    for candidate in [
        path.to_string(),
        format!("{path}.bak"),
        format!("{path}.tmp"),
    ] {
        match std::fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub async fn delete(db: &Db, id: &str) -> AppResult<()> {
    let path: Option<String> =
        sqlx::query_scalar("SELECT payload_path FROM snapshots WHERE id = ?")
            .bind(id)
            .fetch_optional(&db.pool)
            .await?;
    let Some(path) = path else {
        return Err(AppError::msg("快照不存在"));
    };
    // 先删文件再删索引：文件删不掉时保留索引，用户还能重试，而不是留下孤儿 payload。
    remove_payload_files(&path)?;
    sqlx::query("DELETE FROM snapshots WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 保留策略：只淘汰最旧的自动快照，保护快照与手动快照永不淘汰。
pub async fn prune(db: &Db, workspace_id: &str, keep: usize) -> AppResult<usize> {
    let keep = keep.max(1);
    let rows = sqlx::query(
        "SELECT id, payload_path FROM snapshots
         WHERE workspace_id = ? AND snapshot_type = 'auto' AND is_protected = 0
         ORDER BY captured_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(&db.pool)
    .await?;

    if rows.len() <= keep {
        return Ok(0);
    }

    let mut removed = 0usize;
    for row in rows.into_iter().skip(keep) {
        let id: String = row.get("id");
        let path: String = row.get("payload_path");
        remove_payload_files(&path)?;
        sqlx::query("DELETE FROM snapshots WHERE id = ?")
            .bind(&id)
            .execute(&db.pool)
            .await?;
        removed += 1;
    }
    Ok(removed)
}

// ---- 当前激活工作空间（自动快照归属） ----

pub async fn active_workspace(db: &Db) -> AppResult<Option<String>> {
    let v: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM app_settings WHERE key = 'active_workspace'")
            .fetch_optional(&db.pool)
            .await?;
    Ok(v.filter(|id| !id.trim().is_empty()))
}

pub async fn set_active_workspace(db: &Db, workspace_id: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES ('active_workspace', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                        updated_at = excluded.updated_at",
    )
    .bind(workspace_id)
    .bind(now_ms())
    .execute(&db.pool)
    .await?;
    Ok(())
}

// ---- 差异比较 ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDiff {
    pub change: String,
    pub title_a: String,
    pub title_b: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub added: usize,
    pub removed: usize,
    pub moved: usize,
    pub state_changed: usize,
    pub unchanged: usize,
    pub windows: Vec<WindowDiff>,
    pub display_changed: bool,
    pub display_detail: String,
}

fn window_key(w: &crate::core::models::SceneWindow) -> String {
    format!(
        "{}|{}|{}",
        w.process_path
            .as_ref()
            .map(|p| p.to_lowercase())
            .unwrap_or_else(|| format!("proc:{}", w.process_name.to_lowercase())),
        w.class_name,
        normalize_title(&w.title)
    )
}

/// 差异按「进程 + 类名 + 归一化标题」对齐；位置变化按比例坐标的 2% 阈值判定
pub fn diff(a: &SceneFile, b: &SceneFile) -> SnapshotDiff {
    const MOVE_EPS: f64 = 0.02;

    let mut left: Vec<(String, &crate::core::models::SceneWindow)> =
        a.windows.iter().map(|w| (window_key(w), w)).collect();
    let mut right: Vec<(String, &crate::core::models::SceneWindow)> =
        b.windows.iter().map(|w| (window_key(w), w)).collect();

    let mut windows: Vec<WindowDiff> = Vec::new();
    let (mut added, mut removed, mut moved, mut state_changed, mut unchanged) = (0, 0, 0, 0, 0);

    // 先配对同名窗口
    let mut i = 0;
    while i < left.len() {
        let key = left[i].0.clone();
        if let Some(pos) = right.iter().position(|(k, _)| *k == key) {
            let (_, x) = left.remove(i);
            let (_, y) = right.remove(pos);

            let dx = (x.x_ratio - y.x_ratio).abs();
            let dy = (x.y_ratio - y.y_ratio).abs();
            let dw = (x.width_ratio - y.width_ratio).abs();
            let dh = (x.height_ratio - y.height_ratio).abs();

            let state_diff = x.show_state != y.show_state;
            let moved_diff = dx > MOVE_EPS || dy > MOVE_EPS || dw > MOVE_EPS || dh > MOVE_EPS;
            if state_diff {
                state_changed += 1;
            }
            if moved_diff {
                moved += 1;
            }
            if state_diff || moved_diff {
                let mut detail = String::new();
                if state_diff {
                    detail.push_str(&format!("{:?} → {:?}", x.show_state, y.show_state));
                }
                if moved_diff {
                    if !detail.is_empty() {
                        detail.push('；');
                    }
                    detail.push_str(&format!(
                        "位置 ({:.1}%, {:.1}%) → ({:.1}%, {:.1}%)，尺寸 {:.0}%×{:.0}% → {:.0}%×{:.0}%",
                        x.x_ratio * 100.0,
                        x.y_ratio * 100.0,
                        y.x_ratio * 100.0,
                        y.y_ratio * 100.0,
                        x.width_ratio * 100.0,
                        x.height_ratio * 100.0,
                        y.width_ratio * 100.0,
                        y.height_ratio * 100.0
                    ));
                }
                windows.push(WindowDiff {
                    change: if state_diff {
                        "state_changed".into()
                    } else {
                        "moved".into()
                    },
                    title_a: x.title.clone(),
                    title_b: y.title.clone(),
                    detail,
                });
            } else {
                unchanged += 1;
            }
        } else {
            i += 1;
        }
    }

    for (_, w) in &left {
        removed += 1;
        windows.push(WindowDiff {
            change: "removed".into(),
            title_a: w.title.clone(),
            title_b: String::new(),
            detail: format!("{} 已不在较新的快照中", w.process_name),
        });
    }
    for (_, w) in &right {
        added += 1;
        windows.push(WindowDiff {
            change: "added".into(),
            title_a: String::new(),
            title_b: w.title.clone(),
            detail: format!("{} 是新增窗口", w.process_name),
        });
    }

    let displays_a: Vec<(String, i32, i32)> = a
        .displays
        .iter()
        .map(|d| (d.monitor_key.clone(), d.width, d.height))
        .collect();
    let displays_b: Vec<(String, i32, i32)> = b
        .displays
        .iter()
        .map(|d| (d.monitor_key.clone(), d.width, d.height))
        .collect();
    let display_changed = displays_a != displays_b;

    let display_detail = if a.displays.len() != b.displays.len() {
        format!("显示器数量 {} → {}", a.displays.len(), b.displays.len())
    } else if display_changed {
        "显示器分辨率或标识发生变化".to_string()
    } else {
        "显示器无变化".to_string()
    };

    SnapshotDiff {
        added,
        removed,
        moved,
        state_changed,
        unchanged,
        windows,
        display_changed,
        display_detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{SceneWindow, ShowState};

    fn scene(windows: Vec<SceneWindow>) -> SceneFile {
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

    fn w(title: &str, x: f64, state: ShowState) -> SceneWindow {
        SceneWindow {
            process_path: Some(r"C:\app\a.exe".into()),
            process_name: "a.exe".into(),
            class_name: "Cls".into(),
            title: title.into(),
            monitor_key: "M1".into(),
            x_ratio: x,
            y_ratio: 0.1,
            width_ratio: 0.5,
            height_ratio: 0.5,
            original_x: 0,
            original_y: 0,
            original_width: 100,
            original_height: 100,
            source_dpi: 96,
            show_state: state,
            z_order: 0,
            reuse_policy: String::new(),
            launch_args: None,
            launch_priority: 0,
            launch_delay_ms: 0,
        }
    }

    #[test]
    fn diff_detects_added_removed_moved_and_state() {
        let a = scene(vec![
            w("A", 0.10, ShowState::Normal),
            w("B", 0.20, ShowState::Normal),
            w("C", 0.30, ShowState::Maximized),
        ]);
        let b = scene(vec![
            w("A", 0.10, ShowState::Normal), // 未变
            w("B", 0.60, ShowState::Normal), // 移动
            w("C", 0.30, ShowState::Normal), // 状态变化
            w("D", 0.40, ShowState::Normal), // 新增
        ]);

        let d = diff(&a, &b);
        assert_eq!(d.unchanged, 1);
        assert_eq!(d.moved, 1);
        assert_eq!(d.state_changed, 1);
        assert_eq!(d.added, 1);
        assert_eq!(d.removed, 0);
    }

    #[test]
    fn diff_ignores_punctuation_only_title_changes() {
        let a = scene(vec![w("README.md - Code", 0.1, ShowState::Normal)]);
        let b = scene(vec![w("README.md  —  Code", 0.1, ShowState::Normal)]);
        let d = diff(&a, &b);
        assert_eq!(d.unchanged, 1);
        assert_eq!(d.added + d.removed, 0);
    }
}
