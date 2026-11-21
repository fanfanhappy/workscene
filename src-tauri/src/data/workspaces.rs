use crate::core::models::{CaptureResult, SceneFile, SceneResource, SceneWindow, ShowState};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::restore::layout::{to_ratios, Area};
use crate::storage;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_favorite: bool,
    pub item_count: i64,
    pub resource_count: i64,
    pub window_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_restored_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub id: String,
    pub item_type: String,
    pub display_name: String,
    pub target: Option<String>,
    pub target_kind: String,
    pub enabled: bool,
    pub reuse_policy: String,
    pub launch_args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceWindow {
    pub id: String,
    pub item_id: String,
    pub title: String,
    pub class_name: String,
    pub process_path: Option<String>,
    pub process_name: String,
    pub monitor_key: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
    pub source_dpi: Option<i64>,
    pub show_state: ShowState,
    pub z_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_favorite: bool,
    pub auto_snapshot_enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_restored_at: Option<i64>,
    pub items: Vec<WorkspaceItem>,
    pub windows: Vec<WorkspaceWindow>,
}

fn state_to_str(s: ShowState) -> &'static str {
    match s {
        ShowState::Normal => "normal",
        ShowState::Maximized => "maximized",
        ShowState::Minimized => "minimized",
    }
}

fn str_to_state(s: &str) -> ShowState {
    match s {
        "maximized" => ShowState::Maximized,
        "minimized" => ShowState::Minimized,
        _ => ShowState::Normal,
    }
}

/// 稳定身份：可执行路径（小写）优先，路径不可读时退化为进程名
fn identity_of(process_path: &Option<String>, process_name: &str) -> String {
    match process_path {
        Some(p) if !p.trim().is_empty() => p.to_lowercase(),
        _ => format!("proc:{}", process_name.to_lowercase()),
    }
}

pub fn fingerprint(capture: &CaptureResult) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut parts: Vec<String> = capture
        .windows
        .iter()
        .map(|w| {
            format!(
                "{}|{}|{}|{}|{}x{}|{}|{}",
                identity_of(&w.process_path, &w.process_name),
                w.class_name,
                w.monitor_key.clone().unwrap_or_default(),
                (w.x as f64 / 40.0).round() as i64,
                w.width,
                w.height,
                (w.y as f64 / 40.0).round() as i64,
                state_to_str(w.show_state)
            )
        })
        .collect();
    parts.sort();

    let mut hasher = DefaultHasher::new();
    parts.join(";").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 从一次捕获创建工作空间：应用去重为 item，窗口逐条存入 window_layouts，
/// 同时把捕获内容落成一份快照（JSON payload）供历史与撤销使用。
pub async fn create_from_capture(
    db: &Db,
    name: &str,
    capture: &CaptureResult,
) -> AppResult<WorkspaceSummary> {
    if capture.windows.is_empty() {
        return Err(AppError::msg("当前没有可保存的窗口，请先打开需要记录的应用"));
    }

    let name = name.trim();
    let name = if name.is_empty() { "未命名工作空间" } else { name };
    let now = now_ms();
    let ws_id = format!("ws-{:x}", now);
    let mut tx = db.pool.begin().await?;

    sqlx::query(
        "INSERT INTO workspaces (id, name, description, is_favorite, sort_order,
            auto_snapshot_enabled, created_at, updated_at)
         VALUES (?, ?, NULL, 0, 0, 1, ?, ?)",
    )
    .bind(&ws_id)
    .bind(name)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 应用清单：同一身份多个窗口只保留一个 item（PRD 的 workspace_items 1 : N window_layouts）
    let mut item_ids: Vec<(String, String)> = Vec::new(); // (identity, item_id)
    for w in &capture.windows {
        let identity = identity_of(&w.process_path, &w.process_name);
        if item_ids.iter().any(|(k, _)| *k == identity) {
            continue;
        }
        let item_id = format!("it-{:x}-{}", now, item_ids.len());
        let display_name = if w.process_name.is_empty() {
            w.title.clone()
        } else {
            w.process_name.clone()
        };
        sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, 'application', ?, ?, ?, 'path', 1, ?, ?, ?)",
        )
        .bind(&item_id)
        .bind(&ws_id)
        .bind(&display_name)
        .bind(&identity)
        .bind(&w.process_path)
        .bind(item_ids.len() as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO app_configs (item_id, reuse_policy, wait_timeout_ms)
             VALUES (?, 'reuse_or_launch', 8000)",
        )
        .bind(&item_id)
        .execute(&mut *tx)
        .await?;

        item_ids.push((identity, item_id));
    }

    for (idx, w) in capture.windows.iter().enumerate() {
        let identity = identity_of(&w.process_path, &w.process_name);
        let item_id = item_ids
            .iter()
            .find(|(k, _)| *k == identity)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| AppError::msg("内部错误：窗口找不到对应应用项"))?;

        let monitor_key = w.monitor_key.clone().unwrap_or_default();
        let work = capture
            .displays
            .iter()
            .find(|d| d.monitor_key == monitor_key)
            .or_else(|| capture.displays.iter().find(|d| d.is_primary))
            .or_else(|| capture.displays.first());

        let (xr, yr, wr, hr) = match work {
            Some(d) => crate::restore::layout::normalize_ratios(to_ratios(
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

        sqlx::query(
            "INSERT INTO window_layouts (id, workspace_id, item_id, monitor_key,
                x_ratio, y_ratio, width_ratio, height_ratio,
                original_x, original_y, original_width, original_height, source_dpi,
                show_state, z_order, title, class_name, captured_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("wl-{:x}-{}", now, idx))
        .bind(&ws_id)
        .bind(&item_id)
        .bind(&monitor_key)
        .bind(xr)
        .bind(yr)
        .bind(wr)
        .bind(hr)
        .bind(w.x as i64)
        .bind(w.y as i64)
        .bind(w.width as i64)
        .bind(w.height as i64)
        .bind(w.dpi as i64)
        .bind(state_to_str(w.show_state))
        .bind(idx as i64)
        .bind(&w.title)
        .bind(&w.class_name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    // 快照 payload（不可变 JSON），并在 DB 里登记索引与指纹
    let scene = storage::save_scene(name, capture)?;
    sqlx::query(
        "INSERT INTO snapshots (id, workspace_id, name, snapshot_type, trigger_type,
            captured_at, window_count, display_count, payload_path, content_fingerprint,
            payload_version, is_protected)
         VALUES (?, ?, ?, 'initial', 'capture', ?, ?, ?, ?, ?, 1, 1)",
    )
    .bind(&scene.id)
    .bind(&ws_id)
    .bind(name)
    .bind(capture.captured_at)
    .bind(capture.windows.len() as i64)
    .bind(capture.displays.len() as i64)
    .bind(&scene.path)
    .bind(fingerprint(capture))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(WorkspaceSummary {
        id: ws_id,
        name: name.to_string(),
        description: None,
        is_favorite: false,
        item_count: item_ids.len() as i64,
        resource_count: 0,
        window_count: capture.windows.len() as i64,
        created_at: now,
        updated_at: now,
        last_restored_at: None,
    })
}

/// 挂一份快照到工作空间：payload 落 JSON，DB 只存索引与指纹。
/// `snapshot_type` 取值：initial / manual / protection / auto
pub async fn add_snapshot(
    db: &Db,
    workspace_id: &str,
    name: &str,
    snapshot_type: &str,
    trigger: &str,
    capture: &CaptureResult,
    protected: bool,
) -> AppResult<String> {
    let scene = storage::save_scene(name, capture)?;
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
    .bind(fingerprint(capture))
    .bind(if protected { 1 } else { 0 })
    .execute(&db.pool)
    .await?;
    Ok(scene.id)
}

pub async fn list(db: &Db) -> AppResult<Vec<WorkspaceSummary>> {
    let rows = sqlx::query(
        "SELECT w.id, w.name, w.description, w.is_favorite, w.created_at, w.updated_at,
                w.last_restored_at,
                (SELECT COUNT(*) FROM workspace_items i WHERE i.workspace_id = w.id AND i.item_type = 'application') AS item_count,
                (SELECT COUNT(*) FROM workspace_items i WHERE i.workspace_id = w.id AND i.item_type <> 'application') AS resource_count,
                (SELECT COUNT(*) FROM window_layouts l WHERE l.workspace_id = w.id) AS window_count
         FROM workspaces w
         ORDER BY w.is_favorite DESC, w.updated_at DESC",
    )
    .fetch_all(&db.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| WorkspaceSummary {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            is_favorite: r.get::<i64, _>("is_favorite") != 0,
            item_count: r.get("item_count"),
            resource_count: r.get("resource_count"),
            window_count: r.get("window_count"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            last_restored_at: r.get("last_restored_at"),
        })
        .collect())
}

pub async fn get(db: &Db, id: &str) -> AppResult<WorkspaceDetail> {
    let row = sqlx::query(
        "SELECT id, name, description, color, icon, is_favorite, auto_snapshot_enabled,
                created_at, updated_at, last_restored_at
         FROM workspaces WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::msg(format!("工作空间不存在：{id}")))?;

    let item_rows = sqlx::query(
        "SELECT i.id, i.item_type, i.display_name, i.target, i.target_kind, i.enabled,
                COALESCE(c.reuse_policy, 'reuse_or_launch') AS reuse_policy,
                c.launch_args_json
         FROM workspace_items i
         LEFT JOIN app_configs c ON c.item_id = i.id
         WHERE i.workspace_id = ?
         ORDER BY i.sort_order",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?;

    let items: Vec<WorkspaceItem> = item_rows
        .into_iter()
        .map(|r| WorkspaceItem {
            id: r.get("id"),
            item_type: r.get("item_type"),
            display_name: r.get("display_name"),
            target: r.get("target"),
            target_kind: r.get("target_kind"),
            enabled: r.get::<i64, _>("enabled") != 0,
            reuse_policy: r.get("reuse_policy"),
            launch_args: r.get("launch_args_json"),
        })
        .collect();

    let window_rows = sqlx::query(
        "SELECT l.id, l.item_id, l.title, l.class_name, l.monitor_key,
                l.x_ratio, l.y_ratio, l.width_ratio, l.height_ratio, l.source_dpi,
                l.show_state, l.z_order, i.target AS process_path, i.display_name AS process_name
         FROM window_layouts l
         JOIN workspace_items i ON i.id = l.item_id
         WHERE l.workspace_id = ?
         ORDER BY l.z_order",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?;

    let windows: Vec<WorkspaceWindow> = window_rows
        .into_iter()
        .map(|r| {
            let state: String = r.get("show_state");
            WorkspaceWindow {
                id: r.get("id"),
                item_id: r.get("item_id"),
                title: r.get("title"),
                class_name: r.get("class_name"),
                process_path: r.get("process_path"),
                process_name: r.get("process_name"),
                monitor_key: r.get("monitor_key"),
                x_ratio: r.get("x_ratio"),
                y_ratio: r.get("y_ratio"),
                width_ratio: r.get("width_ratio"),
                height_ratio: r.get("height_ratio"),
                source_dpi: r.get("source_dpi"),
                show_state: str_to_state(&state),
                z_order: r.get("z_order"),
            }
        })
        .collect();

    Ok(WorkspaceDetail {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        color: row.get("color"),
        icon: row.get("icon"),
        is_favorite: row.get::<i64, _>("is_favorite") != 0,
        auto_snapshot_enabled: row.get::<i64, _>("auto_snapshot_enabled") != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_restored_at: row.get("last_restored_at"),
        items,
        windows,
    })
}

pub async fn rename(db: &Db, id: &str, name: &str) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("名称不能为空"));
    }
    let now = now_ms();
    let affected = sqlx::query("UPDATE workspaces SET name = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(now)
        .bind(id)
        .execute(&db.pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::msg(format!("工作空间不存在：{id}")));
    }
    Ok(())
}

/// 删除工作空间：级联删除 items / layouts / snapshots（评审 D3），
/// 并清理对应的快照 payload 文件，避免留下孤儿 JSON。
pub async fn delete(db: &Db, id: &str) -> AppResult<()> {
    let payloads: Vec<String> =
        sqlx::query_scalar("SELECT payload_path FROM snapshots WHERE workspace_id = ?")
            .bind(id)
            .fetch_all(&db.pool)
            .await?;

    let affected = sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::msg(format!("工作空间不存在：{id}")));
    }

    for p in payloads {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

/// 更新应用项配置：复用策略、启动参数、启用状态。
/// `launch_args` 以原始字符串存进 `launch_args_json`（不使用 shell，切分在恢复侧做）。
pub async fn update_item(
    db: &Db,
    item_id: &str,
    reuse_policy: &str,
    launch_args: Option<String>,
    enabled: bool,
) -> AppResult<()> {
    let policy = match reuse_policy {
        "reuse_if_matched" | "always_launch" | "reuse_or_launch" => reuse_policy,
        other => return Err(AppError::msg(format!("未知的复用策略：{other}"))),
    };

    let workspace_id: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM workspace_items WHERE id = ?")
            .bind(item_id)
            .fetch_optional(&db.pool)
            .await?;
    let workspace_id = workspace_id.ok_or_else(|| AppError::msg("应用项不存在"))?;

    let now = now_ms();
    let mut tx = db.pool.begin().await?;

    sqlx::query("UPDATE workspace_items SET enabled = ?, updated_at = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(now)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO app_configs (item_id, reuse_policy, launch_args_json, wait_timeout_ms)
         VALUES (?, ?, ?, 8000)
         ON CONFLICT(item_id) DO UPDATE SET
             reuse_policy = excluded.reuse_policy,
             launch_args_json = excluded.launch_args_json",
    )
    .bind(item_id)
    .bind(policy)
    .bind(&launch_args)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(&workspace_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// 布局编辑：一次提交多个窗口的新比例坐标（整体一个事务）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutUpdate {
    pub window_id: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
}

/// 最小可编辑尺寸，防止把窗口拖成一条线（与 layout 保持一致）
pub use crate::restore::layout::MIN_RATIO;

/// 归一化 + 边界约束。UI 拖拽与直接调用命令都要过这一层。
pub fn sanitize_layout(u: &LayoutUpdate) -> (f64, f64, f64, f64) {
    let clamp01 = |v: f64| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let x = clamp01(u.x_ratio).min(1.0 - MIN_RATIO);
    let y = clamp01(u.y_ratio).min(1.0 - MIN_RATIO);
    let max_w = (1.0 - x).max(MIN_RATIO);
    let max_h = (1.0 - y).max(MIN_RATIO);
    let w = clamp01(u.width_ratio).clamp(MIN_RATIO, max_w);
    let h = clamp01(u.height_ratio).clamp(MIN_RATIO, max_h);
    (x, y, w, h)
}

pub async fn update_layouts(
    db: &Db,
    workspace_id: &str,
    updates: &[LayoutUpdate],
) -> AppResult<()> {
    if updates.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    let mut tx = db.pool.begin().await?;

    for u in updates {
        let (x, y, w, h) = sanitize_layout(u);
        let affected = sqlx::query(
            "UPDATE window_layouts
             SET x_ratio = ?, y_ratio = ?, width_ratio = ?, height_ratio = ?
             WHERE id = ? AND workspace_id = ?",
        )
        .bind(x)
        .bind(y)
        .bind(w)
        .bind(h)
        .bind(&u.window_id)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(AppError::msg(format!(
                "窗口布局不存在或不属于该工作空间：{}",
                u.window_id
            )));
        }
    }

    sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn delete_window(db: &Db, workspace_id: &str, window_id: &str) -> AppResult<()> {
    let affected = sqlx::query(
        "DELETE FROM window_layouts WHERE id = ? AND workspace_id = ?",
    )
    .bind(window_id)
    .bind(workspace_id)
    .execute(&db.pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::msg("窗口布局不存在或不属于该工作空间"));
    }
    Ok(())
}

/// 删除应用项：其窗口布局由外键级联删除（migration 里 ON DELETE CASCADE）
pub async fn delete_item(db: &Db, workspace_id: &str, item_id: &str) -> AppResult<()> {
    let affected = sqlx::query(
        "DELETE FROM workspace_items WHERE id = ? AND workspace_id = ?",
    )
    .bind(item_id)
    .bind(workspace_id)
    .execute(&db.pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::msg("应用项不存在或不属于该工作空间"));
    }
    Ok(())
}

/// 添加一个资源项（文件 / 文件夹 / 网址）。
/// 同一工作空间内相同 target 会被 UNIQUE 约束拦住，这里转成可读错误。
pub async fn add_item(
    db: &Db,
    workspace_id: &str,
    item_type: &str,
    target: &str,
    display_name: Option<&str>,
) -> AppResult<WorkspaceItem> {
    if !matches!(item_type, "file" | "folder" | "url") {
        return Err(AppError::msg(format!("不支持的资源类型：{item_type}")));
    }
    let target = target.trim();
    if target.is_empty() {
        return Err(AppError::msg("资源路径或网址不能为空"));
    }
    if item_type == "url" && !(target.starts_with("http://") || target.starts_with("https://")) {
        return Err(AppError::msg("网址必须以 http:// 或 https:// 开头"));
    }

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE id = ?")
            .bind(workspace_id)
            .fetch_optional(&db.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::msg("工作空间不存在"));
    }

    let identity = target.to_lowercase();
    let duplicate: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspace_items WHERE workspace_id = ? AND item_type = ? AND identity_key = ?",
    )
    .bind(workspace_id)
    .bind(item_type)
    .bind(&identity)
    .fetch_optional(&db.pool)
    .await?;
    if duplicate.is_some() {
        return Err(AppError::msg("该资源已在此工作空间中"));
    }

    let name = display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_display_name(item_type, target));

    let now = now_ms();
    // 同一毫秒内可能连续添加多个资源，用进程内序号保证 id 唯一
    static ITEM_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "it-{:x}-res-{}",
        now,
        ITEM_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let sort: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM workspace_items WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_one(&db.pool)
    .await?;

    sqlx::query(
        "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
            identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(item_type)
    .bind(&name)
    .bind(&identity)
    .bind(target)
    .bind(item_type)
    .bind(sort)
    .bind(now)
    .bind(now)
    .execute(&db.pool)
    .await?;

    sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(workspace_id)
        .execute(&db.pool)
        .await?;

    Ok(WorkspaceItem {
        id,
        item_type: item_type.to_string(),
        display_name: name,
        target: Some(target.to_string()),
        target_kind: item_type.to_string(),
        enabled: true,
        reuse_policy: "reuse_or_launch".into(),
        launch_args: None,
    })
}

fn default_display_name(item_type: &str, target: &str) -> String {
    match item_type {
        "url" => target
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string(),
        _ => std::path::Path::new(target)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string()),
    }
}

/// 由工作空间当前布局构造可恢复的 SceneFile（恢复引擎的唯一输入）
pub async fn to_scene(db: &Db, id: &str) -> AppResult<SceneFile> {
    let detail = get(db, id).await?;
    if detail.windows.is_empty() && detail.items.iter().all(|i| i.item_type == "application") {
        return Err(AppError::msg("该工作空间没有可恢复的窗口或资源"));
    }

    // 文件 / 文件夹 / 网址：恢复时按默认程序打开（PRD FR-008 / FR-009）
    let resources: Vec<SceneResource> = detail
        .items
        .iter()
        .filter(|i| i.enabled && matches!(i.item_type.as_str(), "file" | "folder" | "url"))
        .filter_map(|i| {
            let target = i.target.clone().unwrap_or_default();
            if target.trim().is_empty() {
                return None;
            }
            Some(SceneResource {
                item_type: i.item_type.clone(),
                target,
                display_name: i.display_name.clone(),
            })
        })
        .collect();

    let windows = detail
        .windows
        .iter()
        .map(|w| {
            // 每个窗口带上所属应用的复用策略与启动参数（M3）
            let item = detail.items.iter().find(|i| i.id == w.item_id);
            SceneWindow {
                process_path: w.process_path.clone(),
                process_name: w.process_name.clone(),
                class_name: w.class_name.clone(),
                title: w.title.clone(),
                monitor_key: w.monitor_key.clone(),
                x_ratio: w.x_ratio,
                y_ratio: w.y_ratio,
                width_ratio: w.width_ratio,
                height_ratio: w.height_ratio,
                original_x: 0,
                original_y: 0,
                original_width: 0,
                original_height: 0,
                source_dpi: w.source_dpi.unwrap_or(96) as u32,
                show_state: w.show_state,
                z_order: w.z_order as i32,
                reuse_policy: item
                    .map(|i| i.reuse_policy.clone())
                    .unwrap_or_else(|| "reuse_or_launch".into()),
                launch_args: item.and_then(|i| i.launch_args.clone()),
            }
        })
        .collect();

    Ok(SceneFile {
        schema_version: storage::SCHEMA_VERSION,
        id: detail.id.clone(),
        name: detail.name.clone(),
        captured_at: detail.updated_at,
        displays: Vec::new(),
        windows,
        resources,
    })
}

pub async fn mark_restored(db: &Db, id: &str) -> AppResult<()> {
    sqlx::query("UPDATE workspaces SET last_restored_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 快照属于哪个工作空间（撤销/回退时需要把 run 归档到正确的空间）
pub async fn workspace_of_snapshot(db: &Db, snapshot_id: &str) -> AppResult<Option<String>> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM snapshots WHERE id = ?")
            .bind(snapshot_id)
            .fetch_optional(&db.pool)
            .await?;
    Ok(id)
}

/// 最新快照 id，用于把恢复任务关联到快照（可追溯）
pub async fn latest_snapshot(db: &Db, workspace_id: &str) -> AppResult<Option<String>> {
    let id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM snapshots WHERE workspace_id = ? ORDER BY captured_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{DisplayInfo, WindowInfo};

    fn capture() -> CaptureResult {
        let display = DisplayInfo {
            monitor_key: "M1".into(),
            device_name: "M1".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            work_x: 0,
            work_y: 0,
            work_width: 1920,
            work_height: 1040,
            dpi: 96,
            scale_factor: 1.0,
            is_primary: true,
        };
        let win = |hwnd: usize, title: &str, class: &str, path: &str| WindowInfo {
            hwnd,
            process_id: 10,
            process_path: Some(path.into()),
            process_name: "app.exe".into(),
            process_started_at: Some(1),
            title: title.into(),
            class_name: class.into(),
            x: 100,
            y: 100,
            width: 800,
            height: 600,
            dpi: 96,
            monitor_key: Some("M1".into()),
            is_visible: true,
            is_minimized: false,
            is_maximized: false,
            show_state: ShowState::Normal,
        };
        CaptureResult {
            captured_at: 1_700_000_000_000,
            duration_ms: 5,
            windows: vec![
                win(1, "A", "ClassA", r"C:\app\a.exe"),
                win(2, "B", "ClassA", r"C:\app\a.exe"),
                win(3, "C", "ClassB", r"C:\app\b.exe"),
            ],
            displays: vec![display],
            excluded: vec![],
            diagnostics: vec![],
        }
    }

    /// 测试统一把数据目录指向仓库内的 .testdata：
    /// 受限账户下 %LOCALAPPDATA%\Temp 与本机 %LOCALAPPDATA% 都不可写。
    fn test_env() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("仓库根目录")
                .join(".testdata");
            std::fs::create_dir_all(&dir).expect("创建测试数据目录");
            std::env::set_var("WORKSCENE_DATA_DIR", dir);
        });
    }

    async fn db() -> super::super::Db {
        test_env();
        super::super::in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn create_dedupes_apps_and_keeps_all_windows() {
        let db = db().await;
        let summary = create_from_capture(&db, "开发", &capture()).await.unwrap();
        assert_eq!(summary.window_count, 3);
        assert_eq!(summary.item_count, 2, "两个可执行文件 -> 两个应用项");

        let detail = get(&db, &summary.id).await.unwrap();
        assert_eq!(detail.items.len(), 2);
        assert_eq!(detail.windows.len(), 3);
        assert!(detail.windows.iter().all(|w| w.x_ratio >= 0.0));
    }

    #[tokio::test]
    async fn delete_cascades_items_layouts_and_snapshots() {
        let db = db().await;
        let summary = create_from_capture(&db, "开发", &capture()).await.unwrap();
        delete(&db, &summary.id).await.unwrap();

        let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspace_items")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let layouts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM window_layouts")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let snaps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM snapshots")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!((items, layouts, snaps), (0, 0, 0));
    }

    #[tokio::test]
    async fn duplicate_identity_is_rejected_by_unique_constraint() {
        let db = db().await;
        let s = create_from_capture(&db, "开发", &capture()).await.unwrap();
        let err = sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name, identity_key,
                target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES ('dup', ?, 'application', 'dup', ?, NULL, 'path', 1, 0, 0, 0)",
        )
        .bind(&s.id)
        .bind(r"c:\app\a.exe")
        .execute(&db.pool)
        .await;
        assert!(err.is_err(), "同一工作空间内相同 identity 必须被 UNIQUE 拒绝");
    }

    #[tokio::test]
    async fn to_scene_roundtrips_ratios() {
        let db = db().await;
        let s = create_from_capture(&db, "开发", &capture()).await.unwrap();
        let scene = to_scene(&db, &s.id).await.unwrap();
        assert_eq!(scene.windows.len(), 3);
        let first = &scene.windows[0];
        // 800x600 窗口在 1920x1040 工作区中的比例
        assert!((first.width_ratio - 800.0 / 1920.0).abs() < 1e-6);
        assert_eq!(first.monitor_key, "M1");
    }

    #[tokio::test]
    async fn empty_capture_is_rejected() {
        let db = db().await;
        let mut c = capture();
        c.windows.clear();
        assert!(create_from_capture(&db, "空", &c).await.is_err());
    }

    #[test]
    fn sanitize_layout_clamps_into_unit_square() {
        let (x, y, w, h) = sanitize_layout(&LayoutUpdate {
            window_id: "w".into(),
            x_ratio: -0.5,
            y_ratio: 1.4,
            width_ratio: 0.02,
            height_ratio: 5.0,
        });
        assert_eq!((x, y), (0.0, 1.0 - MIN_RATIO));
        assert!(w >= MIN_RATIO && w <= (1.0 - x) + 1e-9);
        assert!(h >= MIN_RATIO && h <= (1.0 - y) + 1e-9);

        // NaN 不应产生非法值
        let (x, _, w, _) = sanitize_layout(&LayoutUpdate {
            window_id: "w".into(),
            x_ratio: f64::NAN,
            y_ratio: 0.0,
            width_ratio: f64::NAN,
            height_ratio: 0.2,
        });
        assert_eq!(x, 0.0);
        assert!(w >= MIN_RATIO);
    }

    #[tokio::test]
    async fn update_layouts_persists_and_rejects_foreign_window() {
        let db = db().await;
        let s = create_from_capture(&db, "编辑", &capture()).await.unwrap();
        let detail = get(&db, &s.id).await.unwrap();
        let first = &detail.windows[0];

        update_layouts(
            &db,
            &s.id,
            &[LayoutUpdate {
                window_id: first.id.clone(),
                x_ratio: 0.25,
                y_ratio: 0.25,
                width_ratio: 0.5,
                height_ratio: 0.5,
            }],
        )
        .await
        .unwrap();

        let after = get(&db, &s.id).await.unwrap();
        let updated = after.windows.iter().find(|w| w.id == first.id).unwrap();
        assert!((updated.x_ratio - 0.25).abs() < 1e-9);

        let err = update_layouts(
            &db,
            &s.id,
            &[LayoutUpdate {
                window_id: "not-mine".into(),
                x_ratio: 0.1,
                y_ratio: 0.1,
                width_ratio: 0.2,
                height_ratio: 0.2,
            }],
        )
        .await;
        assert!(err.is_err(), "不属于该工作空间的布局必须被拒绝");
    }

    #[tokio::test]
    async fn resources_are_added_deduped_and_included_in_scene() {
        let db = db().await;
        let s = create_from_capture(&db, "资源", &capture()).await.unwrap();

        let file = add_item(&db, &s.id, "file", r"D:\docs\spec.md", None).await.unwrap();
        assert_eq!(file.display_name, "spec.md");

        add_item(&db, &s.id, "folder", r"D:\docs", None).await.unwrap();
        let url = add_item(&db, &s.id, "url", "https://example.com/x", None)
            .await
            .unwrap();
        assert_eq!(url.display_name, "example.com/x");

        // 同一资源重复添加必须被拒绝（UNIQUE + 前置检查）
        assert!(add_item(&db, &s.id, "file", r"d:\docs\SPEC.md", None).await.is_err());
        // 非 http(s) 的网址直接拒绝
        assert!(add_item(&db, &s.id, "url", "file:///etc/passwd", None).await.is_err());
        // 不支持的类型
        assert!(add_item(&db, &s.id, "application", r"C:\a.exe", None).await.is_err());

        let scene = to_scene(&db, &s.id).await.unwrap();
        assert_eq!(scene.resources.len(), 3);
        assert!(scene.resources.iter().any(|r| r.item_type == "url"));
    }

    #[tokio::test]
    async fn delete_item_cascades_its_windows() {
        let db = db().await;
        let s = create_from_capture(&db, "删应用", &capture()).await.unwrap();
        let detail = get(&db, &s.id).await.unwrap();
        let item = &detail.items[0];
        let owned = detail.windows.iter().filter(|w| w.item_id == item.id).count();
        assert!(owned > 0);

        delete_item(&db, &s.id, &item.id).await.unwrap();

        let after = get(&db, &s.id).await.unwrap();
        assert_eq!(after.windows.len(), detail.windows.len() - owned);
        assert!(!after.items.iter().any(|i| i.id == item.id));
    }
}
