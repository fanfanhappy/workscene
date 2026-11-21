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
    pub color: Option<String>,
    pub is_favorite: bool,
    pub item_count: i64,
    pub resource_count: i64,
    pub window_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_restored_at: Option<i64>,
    #[serde(default)]
    pub app_paths: Vec<String>,
    /// Ctrl+Alt+N 绑定的槽位（1..=9），None 表示未绑定。
    /// 持久化而不是按列表位置推算：否则删掉一个空间后快捷键会指到别的空间。
    #[serde(default)]
    pub hotkey_slot: Option<i64>,
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
    #[serde(default)]
    pub launch_priority: i64,
    #[serde(default)]
    pub launch_delay_ms: i64,
}

fn parse_delay_ms(adapter_config_json: Option<&str>) -> i64 {
    let Some(raw) = adapter_config_json else {
        return 0;
    };
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("delayMs").and_then(|d| d.as_i64()))
        .unwrap_or(0)
        .clamp(0, 15_000)
}

fn encode_delay_ms(delay_ms: i64) -> Option<String> {
    let clamped = delay_ms.clamp(0, 15_000);
    if clamped == 0 {
        None
    } else {
        Some(format!(r#"{{"delayMs":{clamped}}}"#))
    }
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

    let mut app_paths: Vec<String> = Vec::new();
    for w in &capture.windows {
        if let Some(p) = &w.process_path {
            if !p.is_empty() && !app_paths.iter().any(|x| x.eq_ignore_ascii_case(p)) {
                app_paths.push(p.clone());
                if app_paths.len() >= 6 {
                    break;
                }
            }
        }
    }

    Ok(WorkspaceSummary {
        id: ws_id,
        name: name.to_string(),
        description: None,
        color: None,
        is_favorite: false,
        item_count: item_ids.len() as i64,
        resource_count: 0,
        window_count: capture.windows.len() as i64,
        created_at: now,
        updated_at: now,
        last_restored_at: None,
        app_paths,
        hotkey_slot: None,
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
        "SELECT w.id, w.name, w.description, w.color, w.is_favorite, w.created_at, w.updated_at,
                w.last_restored_at, w.hotkey_slot,
                (SELECT COUNT(*) FROM workspace_items i WHERE i.workspace_id = w.id AND i.item_type = 'application') AS item_count,
                (SELECT COUNT(*) FROM workspace_items i WHERE i.workspace_id = w.id AND i.item_type <> 'application') AS resource_count,
                (SELECT COUNT(*) FROM window_layouts l WHERE l.workspace_id = w.id) AS window_count
         FROM workspaces w
         ORDER BY w.is_favorite DESC, w.updated_at DESC",
    )
    .fetch_all(&db.pool)
    .await?;

    // 批量查出各空间应用可执行路径（每空间最多 6 个），供卡片展示真实软件图标
    let app_rows = sqlx::query(
        "SELECT workspace_id, target
         FROM workspace_items
         WHERE item_type = 'application' AND enabled = 1 AND target IS NOT NULL AND target <> ''
         ORDER BY sort_order ASC",
    )
    .fetch_all(&db.pool)
    .await?;

    let mut path_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for r in app_rows {
        let ws_id: String = r.get("workspace_id");
        let target: String = r.get("target");
        let entry = path_map.entry(ws_id).or_default();
        if entry.len() < 6 && !entry.iter().any(|x| x.eq_ignore_ascii_case(&target)) {
            entry.push(target);
        }
    }

    Ok(rows
        .into_iter()
        .map(|r| {
            let id: String = r.get("id");
            let app_paths = path_map.remove(&id).unwrap_or_default();
            WorkspaceSummary {
                id,
                name: r.get("name"),
                description: r.get("description"),
                color: r.get("color"),
                is_favorite: r.get::<i64, _>("is_favorite") != 0,
                item_count: r.get("item_count"),
                resource_count: r.get("resource_count"),
                window_count: r.get("window_count"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                last_restored_at: r.get("last_restored_at"),
                hotkey_slot: r.get::<Option<i64>, _>("hotkey_slot"),
                app_paths,
            }
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
                i.sort_order,
                COALESCE(c.reuse_policy, 'reuse_or_launch') AS reuse_policy,
                c.launch_args_json,
                c.adapter_config_json
         FROM workspace_items i
         LEFT JOIN app_configs c ON c.item_id = i.id
         WHERE i.workspace_id = ?
         ORDER BY i.sort_order ASC, i.created_at ASC",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?;

    let items: Vec<WorkspaceItem> = item_rows
        .into_iter()
        .map(|r| {
            let adapter_cfg: Option<String> = r.get("adapter_config_json");
            WorkspaceItem {
                id: r.get("id"),
                item_type: r.get("item_type"),
                display_name: r.get("display_name"),
                target: r.get("target"),
                target_kind: r.get("target_kind"),
                enabled: r.get::<i64, _>("enabled") != 0,
                reuse_policy: r.get("reuse_policy"),
                launch_args: r.get("launch_args_json"),
                launch_priority: r.get("sort_order"),
                launch_delay_ms: parse_delay_ms(adapter_cfg.as_deref()),
            }
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

/// 更新应用项配置：复用策略、启动参数、启用状态、可选启动优先级与启动后等待延迟。
/// `launch_args` 以原始字符串存进 `launch_args_json`（不使用 shell，切分在恢复侧做）。
pub async fn update_item(
    db: &Db,
    item_id: &str,
    reuse_policy: &str,
    launch_args: Option<String>,
    enabled: bool,
    launch_priority: Option<i64>,
    launch_delay_ms: Option<i64>,
) -> AppResult<()> {
    let policy = match reuse_policy {
        "reuse_if_matched" | "always_launch" | "reuse_or_launch" => reuse_policy,
        other => return Err(AppError::msg(format!("未知的复用策略：{other}"))),
    };

    let row = sqlx::query(
        "SELECT i.workspace_id, i.sort_order, c.adapter_config_json
         FROM workspace_items i
         LEFT JOIN app_configs c ON c.item_id = i.id
         WHERE i.id = ?",
    )
    .bind(item_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::msg("应用项不存在"))?;

    let workspace_id: String = row.get("workspace_id");
    let sort_order: i64 = launch_priority.unwrap_or_else(|| row.get("sort_order"));
    let existing_cfg: Option<String> = row.get("adapter_config_json");
    let delay_ms = launch_delay_ms
        .map(|d| d.clamp(0, 15_000))
        .unwrap_or_else(|| parse_delay_ms(existing_cfg.as_deref()));
    let adapter_cfg = encode_delay_ms(delay_ms);

    let now = now_ms();
    let mut tx = db.pool.begin().await?;

    sqlx::query("UPDATE workspace_items SET enabled = ?, sort_order = ?, updated_at = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(sort_order)
        .bind(now)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO app_configs (item_id, reuse_policy, launch_args_json, wait_timeout_ms, adapter_config_json)
         VALUES (?, ?, ?, 8000, ?)
         ON CONFLICT(item_id) DO UPDATE SET
             reuse_policy = excluded.reuse_policy,
             launch_args_json = excluded.launch_args_json,
             adapter_config_json = excluded.adapter_config_json",
    )
    .bind(item_id)
    .bind(policy)
    .bind(&launch_args)
    .bind(&adapter_cfg)
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

/// 重新定位应用可执行路径（软件重装、换盘符或导入他人工作空间后快速修复）
pub async fn relocate_item(
    db: &Db,
    workspace_id: &str,
    item_id: &str,
    new_path: &str,
) -> AppResult<WorkspaceItem> {
    let trimmed = new_path.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("程序路径不能为空"));
    }
    if !std::path::Path::new(trimmed).is_file() {
        return Err(AppError::msg(format!("找不到文件：{trimmed}")));
    }

    let row = sqlx::query(
        "SELECT i.item_type, i.display_name, i.target_kind, i.enabled, i.sort_order,
                COALESCE(c.reuse_policy, 'reuse_or_launch') AS reuse_policy,
                c.launch_args_json, c.adapter_config_json
         FROM workspace_items i
         LEFT JOIN app_configs c ON c.item_id = i.id
         WHERE i.id = ? AND i.workspace_id = ?",
    )
    .bind(item_id)
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::msg("应用项不存在或不属于该工作空间"))?;

    let item_type: String = row.get("item_type");
    if item_type != "application" {
        return Err(AppError::msg("只能为应用程序重新定位可执行路径"));
    }

    let new_identity = trimmed.to_lowercase();
    let conflict: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspace_items
         WHERE workspace_id = ? AND item_type = 'application' AND identity_key = ? AND id <> ?",
    )
    .bind(workspace_id)
    .bind(&new_identity)
    .bind(item_id)
    .fetch_optional(&db.pool)
    .await?;
    if conflict.is_some() {
        return Err(AppError::msg("工作空间中已存在指向同一程序的应用项"));
    }

    let display_name = std::path::Path::new(trimmed)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| row.get("display_name"));

    let now = now_ms();
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "UPDATE workspace_items
         SET target = ?, identity_key = ?, display_name = ?, updated_at = ?
         WHERE id = ? AND workspace_id = ?",
    )
    .bind(trimmed)
    .bind(&new_identity)
    .bind(&display_name)
    .bind(now)
    .bind(item_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let adapter_cfg: Option<String> = row.get("adapter_config_json");
    Ok(WorkspaceItem {
        id: item_id.to_string(),
        item_type,
        display_name,
        target: Some(trimmed.to_string()),
        target_kind: row.get("target_kind"),
        enabled: row.get::<i64, _>("enabled") != 0,
        reuse_policy: row.get("reuse_policy"),
        launch_args: row.get("launch_args_json"),
        launch_priority: row.get("sort_order"),
        launch_delay_ms: parse_delay_ms(adapter_cfg.as_deref()),
    })
}

/// 布局编辑：一次提交多个窗口的新比例坐标，可选同步修改目标显示器与显示状态（整体一个事务）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutUpdate {
    pub window_id: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
    #[serde(default)]
    pub monitor_key: Option<String>,
    #[serde(default)]
    pub show_state: Option<ShowState>,
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
        let state_str = u.show_state.map(state_to_str);
        let monitor = u
            .monitor_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let affected = sqlx::query(
            "UPDATE window_layouts
             SET x_ratio = ?, y_ratio = ?, width_ratio = ?, height_ratio = ?,
                 monitor_key = COALESCE(?, monitor_key),
                 show_state = COALESCE(?, show_state)
             WHERE id = ? AND workspace_id = ?",
        )
        .bind(x)
        .bind(y)
        .bind(w)
        .bind(h)
        .bind(monitor)
        .bind(state_str)
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
        launch_priority: sort,
        launch_delay_ms: 0,
    })
}

/// 批量添加网址资源（支持多行粘贴或一键导入 Chrome/Edge 书签文件夹，自动跳过重复）
pub async fn add_urls_batch(
    db: &Db,
    workspace_id: &str,
    entries: &[(String, Option<String>)],
) -> AppResult<usize> {
    if entries.is_empty() {
        return Ok(0);
    }
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(&db.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::msg(format!("工作空间不存在：{workspace_id}")));
    }

    let now = now_ms();
    let mut sort: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM workspace_items WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_one(&db.pool)
    .await?;

    let mut tx = db.pool.begin().await?;
    let mut added = 0usize;

    for (raw_url, custom_name) in entries {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            continue;
        }
        let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };
        let identity = url.to_lowercase();
        let name = custom_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_display_name("url", &url));
        let id = format!(
            "it-{:x}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let res = sqlx::query(
            "INSERT OR IGNORE INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, 'url', ?, ?, ?, 'url', 1, ?, ?, ?)",
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(&name)
        .bind(&identity)
        .bind(&url)
        .bind(sort)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        if res.rows_affected() > 0 {
            added += 1;
            sort += 1;
        }
    }

    if added > 0 {
        sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(workspace_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(added)
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

    let windows: Vec<SceneWindow> = detail
        .windows
        .iter()
        .filter(|w| {
            // 仅恢复已启用应用所属的窗口（尊重 workspace_items.enabled）
            detail
                .items
                .iter()
                .find(|i| i.id == w.item_id)
                .map(|i| i.enabled)
                .unwrap_or(true)
        })
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
                launch_priority: item.map(|i| i.launch_priority).unwrap_or(0),
                launch_delay_ms: item.map(|i| i.launch_delay_ms.max(0) as u64).unwrap_or(0),
            }
        })
        .collect();

    if windows.is_empty() && resources.is_empty() {
        return Err(AppError::msg("该工作空间没有已启用的窗口或资源可恢复"));
    }

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

/// 快捷槽上限（Ctrl+Alt+1..9）
pub const MAX_HOTKEY_SLOT: i64 = 9;

fn validate_slot(slot: Option<i64>) -> AppResult<Option<i64>> {
    match slot {
        None => Ok(None),
        Some(s) if (1..=MAX_HOTKEY_SLOT).contains(&s) => Ok(Some(s)),
        Some(s) => Err(AppError::msg(format!(
            "快捷槽必须是 1..{MAX_HOTKEY_SLOT} 之间的数字，收到 {s}"
        ))),
    }
}

/// 绑定/解绑 Ctrl+Alt+N。
///
/// 槽位独占：绑到已被占用的槽时，把原占用者解绑（而不是报错），
/// 否则用户会遇到"点了没反应又不说为什么"。
pub async fn set_hotkey_slot(db: &Db, id: &str, slot: Option<i64>) -> AppResult<()> {
    let slot = validate_slot(slot)?;
    let mut tx = db.pool.begin().await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(AppError::msg(format!("工作空间不存在：{id}")));
    }

    if let Some(s) = slot {
        sqlx::query("UPDATE workspaces SET hotkey_slot = NULL, updated_at = ? WHERE hotkey_slot = ? AND id <> ?")
            .bind(now_ms())
            .bind(s)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("UPDATE workspaces SET hotkey_slot = ?, updated_at = ? WHERE id = ?")
        .bind(slot)
        .bind(now_ms())
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// 槽位 → 工作空间 id。空槽返回 None（快捷键不应该"恢复到列表第 N 个"）。
pub async fn id_for_slot(db: &Db, slot: i64) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT id FROM workspaces WHERE hotkey_slot = ?")
        .bind(slot)
        .fetch_optional(&db.pool)
        .await
        .ok()
        .flatten()
}
pub async fn set_favorite(db: &Db, id: &str, is_favorite: bool) -> AppResult<()> {
    let affected = sqlx::query(
        "UPDATE workspaces SET is_favorite = ?, updated_at = ? WHERE id = ?",
    )
    .bind(if is_favorite { 1 } else { 0 })
    .bind(now_ms())
    .bind(id)
    .execute(&db.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::msg(format!("工作空间不存在：{id}")));
    }
    Ok(())
}

pub async fn update_meta(
    db: &Db,
    id: &str,
    name: &str,
    description: Option<&str>,
    auto_snapshot_enabled: bool,
    color: Option<&str>,
) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("名称不能为空"));
    }
    let desc = description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let color_val = color
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let affected = sqlx::query(
        "UPDATE workspaces
         SET name = ?, description = ?, auto_snapshot_enabled = ?, color = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(name)
    .bind(desc)
    .bind(if auto_snapshot_enabled { 1 } else { 0 })
    .bind(color_val)
    .bind(now_ms())
    .bind(id)
    .execute(&db.pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::msg(format!("工作空间不存在：{id}")));
    }
    Ok(())
}

/// 进程内唯一 ID 序号，避免同毫秒操作产生主键冲突
static WS_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_ws_id(now: i64) -> String {
    format!(
        "ws-{:x}-{}",
        now,
        WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// 复制工作空间：完整克隆工作空间配置、应用项、启动参数、资源清单与窗口布局
pub async fn duplicate(db: &Db, source_id: &str, new_name: &str) -> AppResult<WorkspaceSummary> {
    let src = get(db, source_id).await?;
    let name = new_name.trim();
    let name = if name.is_empty() {
        format!("{}（副本）", src.name)
    } else {
        name.to_string()
    };

    let now = now_ms();
    let new_ws_id = next_ws_id(now);
    let mut tx = db.pool.begin().await?;

    sqlx::query(
        "INSERT INTO workspaces (id, name, description, color, icon, is_favorite, sort_order,
            auto_snapshot_enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 0, 0, ?, ?, ?)",
    )
    .bind(&new_ws_id)
    .bind(&name)
    .bind(&src.description)
    .bind(&src.color)
    .bind(&src.icon)
    .bind(if src.auto_snapshot_enabled { 1 } else { 0 })
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let mut id_map: Vec<(String, String)> = Vec::new();
    let mut app_count = 0i64;
    let mut res_count = 0i64;

    for (idx, item) in src.items.iter().enumerate() {
        let new_item_id = format!(
            "it-{:x}-{}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        );
        let identity = if item.item_type == "application" {
            identity_of(&item.target, &item.display_name)
        } else {
            item.target.clone().unwrap_or_default().to_lowercase()
        };

        sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&new_item_id)
        .bind(&new_ws_id)
        .bind(&item.item_type)
        .bind(&item.display_name)
        .bind(&identity)
        .bind(&item.target)
        .bind(&item.target_kind)
        .bind(if item.enabled { 1 } else { 0 })
        .bind(idx as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        if item.item_type == "application" {
            app_count += 1;
            sqlx::query(
                "INSERT INTO app_configs (item_id, reuse_policy, launch_args_json, wait_timeout_ms)
                 VALUES (?, ?, ?, 8000)",
            )
            .bind(&new_item_id)
            .bind(&item.reuse_policy)
            .bind(&item.launch_args)
            .execute(&mut *tx)
            .await?;
        } else {
            res_count += 1;
        }

        id_map.push((item.id.clone(), new_item_id));
    }

    for (idx, w) in src.windows.iter().enumerate() {
        let Some((_, new_item_id)) = id_map.iter().find(|(old, _)| old == &w.item_id) else {
            continue;
        };
        let new_win_id = format!(
            "wl-{:x}-{}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        );
        sqlx::query(
            "INSERT INTO window_layouts (id, workspace_id, item_id, monitor_key,
                x_ratio, y_ratio, width_ratio, height_ratio, source_dpi,
                show_state, z_order, title, class_name, captured_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&new_win_id)
        .bind(&new_ws_id)
        .bind(new_item_id)
        .bind(&w.monitor_key)
        .bind(w.x_ratio)
        .bind(w.y_ratio)
        .bind(w.width_ratio)
        .bind(w.height_ratio)
        .bind(w.source_dpi)
        .bind(state_to_str(w.show_state))
        .bind(w.z_order)
        .bind(&w.title)
        .bind(&w.class_name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let app_paths: Vec<String> = src
        .items
        .iter()
        .filter(|i| i.item_type == "application" && i.enabled)
        .filter_map(|i| i.target.clone())
        .filter(|p| !p.is_empty())
        .take(6)
        .collect();

    Ok(WorkspaceSummary {
        id: new_ws_id,
        name,
        description: src.description,
        color: src.color,
        is_favorite: false,
        item_count: app_count,
        resource_count: res_count,
        window_count: src.windows.len() as i64,
        created_at: now,
        updated_at: now,
        last_restored_at: None,
        app_paths,
        hotkey_slot: None,
    })
}

/// 可移植的工作空间导出格式（PRD §19 导入导出）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceExport {
    pub schema_version: u32,
    pub exported_at: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub auto_snapshot_enabled: bool,
    pub apps: Vec<ExportApp>,
    pub resources: Vec<ExportResource>,
    pub windows: Vec<ExportWindow>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportApp {
    pub display_name: String,
    pub target: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_reuse_policy")]
    pub reuse_policy: String,
    #[serde(default)]
    pub launch_args: Option<String>,
}

fn default_reuse_policy() -> String {
    "reuse_or_launch".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResource {
    pub item_type: String,
    pub display_name: String,
    pub target: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWindow {
    pub app_index: usize,
    pub title: String,
    pub class_name: String,
    pub monitor_key: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
    #[serde(default)]
    pub source_dpi: Option<i64>,
    #[serde(default)]
    pub show_state: ShowState,
    #[serde(default)]
    pub z_order: i64,
}

pub async fn export_json(db: &Db, id: &str) -> AppResult<String> {
    let detail = get(db, id).await?;
    let app_items: Vec<&WorkspaceItem> = detail
        .items
        .iter()
        .filter(|i| i.item_type == "application")
        .collect();

    let apps: Vec<ExportApp> = app_items
        .iter()
        .map(|a| ExportApp {
            display_name: a.display_name.clone(),
            target: a.target.clone(),
            enabled: a.enabled,
            reuse_policy: a.reuse_policy.clone(),
            launch_args: a.launch_args.clone(),
        })
        .collect();

    let resources: Vec<ExportResource> = detail
        .items
        .iter()
        .filter(|i| matches!(i.item_type.as_str(), "file" | "folder" | "url"))
        .filter_map(|r| {
            let target = r.target.clone()?;
            Some(ExportResource {
                item_type: r.item_type.clone(),
                display_name: r.display_name.clone(),
                target,
                enabled: r.enabled,
            })
        })
        .collect();

    let windows: Vec<ExportWindow> = detail
        .windows
        .iter()
        .filter_map(|w| {
            let app_index = app_items.iter().position(|a| a.id == w.item_id)?;
            Some(ExportWindow {
                app_index,
                title: w.title.clone(),
                class_name: w.class_name.clone(),
                monitor_key: w.monitor_key.clone(),
                x_ratio: w.x_ratio,
                y_ratio: w.y_ratio,
                width_ratio: w.width_ratio,
                height_ratio: w.height_ratio,
                source_dpi: w.source_dpi,
                show_state: w.show_state,
                z_order: w.z_order,
            })
        })
        .collect();

    let payload = WorkspaceExport {
        schema_version: 1,
        exported_at: now_ms(),
        name: detail.name,
        description: detail.description,
        auto_snapshot_enabled: detail.auto_snapshot_enabled,
        apps,
        resources,
        windows,
    };

    Ok(serde_json::to_string_pretty(&payload)?)
}

/// 导入时对可执行文件目标的信任评估。
///
/// 工作空间 JSON 可能来自外部（同事分享、网盘下载），其中 `target` 与
/// `launch_args` 都是"要执行的指令"。因此导入必须做三件事：
/// 1. 不认识的路径不自动启用；
/// 2. 启动参数默认剥离（用户确认后再手动加）；
/// 3. 把风险写回工作空间描述，让用户在界面上看得见。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRisk {
    Ok,
    Empty,
    Missing,
    /// UNC 路径（\\server\share）——可能指向别人机器上的程序
    Network,
}

pub fn classify_target(target: &str) -> TargetRisk {
    let t = target.trim();
    if t.is_empty() {
        return TargetRisk::Empty;
    }
    if t.starts_with(r"\\") {
        return TargetRisk::Network;
    }
    let p = std::path::Path::new(t);
    if !p.is_file() {
        return TargetRisk::Missing;
    }
    TargetRisk::Ok
}

/// 导入预览（供 UI 展示"这份文件会启动什么"，目前仅后端可用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub name: String,
    pub app_count: usize,
    pub resource_count: usize,
    /// 会因路径不存在/网络位置而被停用的应用
    pub disabled_apps: Vec<String>,
    /// 原始文件里带启动参数的应用名（导入时会被剥离）
    pub apps_with_args: Vec<String>,
}

pub fn import_preview(raw_json: &str) -> AppResult<ImportPreview> {
    if raw_json.len() > 2 * 1024 * 1024 {
        return Err(AppError::msg("导入文件过大（上限 2 MB）"));
    }
    let parsed: WorkspaceExport = serde_json::from_str(raw_json)
        .map_err(|e| AppError::msg(format!("无效的工作空间 JSON 文件：{e}")))?;

    let disabled_apps = parsed
        .apps
        .iter()
        .filter(|a| classify_target(a.target.as_deref().unwrap_or("")) != TargetRisk::Ok)
        .map(|a| a.display_name.clone())
        .collect();

    let apps_with_args = parsed
        .apps
        .iter()
        .filter(|a| a.launch_args.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false))
        .map(|a| a.display_name.clone())
        .collect();

    Ok(ImportPreview {
        name: parsed.name.clone(),
        app_count: parsed.apps.len(),
        resource_count: parsed.resources.len(),
        disabled_apps,
        apps_with_args,
    })
}

pub async fn import_json(db: &Db, raw_json: &str) -> AppResult<WorkspaceSummary> {
    if raw_json.len() > 2 * 1024 * 1024 {
        return Err(AppError::msg("导入文件过大（上限 2 MB）"));
    }
    let parsed: WorkspaceExport = serde_json::from_str(raw_json)
        .map_err(|e| AppError::msg(format!("无效的工作空间 JSON 文件：{e}")))?;

    if parsed.schema_version == 0 || parsed.schema_version > 1 {
        return Err(AppError::msg(format!(
            "不支持的工作空间导入版本（schemaVersion = {}）",
            parsed.schema_version
        )));
    }
    let name = parsed.name.trim();
    if name.is_empty() {
        return Err(AppError::msg("导入文件中的工作空间名称不能为空"));
    }
    if parsed.apps.is_empty() && parsed.resources.is_empty() {
        return Err(AppError::msg("导入文件不包含任何应用或资源"));
    }

    let now = now_ms();
    let ws_id = next_ws_id(now);
    let mut tx = db.pool.begin().await?;

    // 先把每个应用的信任状态算出来，才能把风险写进工作空间描述
    let app_risks: Vec<TargetRisk> = parsed
        .apps
        .iter()
        .map(|a| classify_target(a.target.as_deref().unwrap_or("")))
        .collect();
    let mut disabled_apps: Vec<String> = Vec::new();
    let mut stripped_args = 0usize;
    for (app, risk) in parsed.apps.iter().zip(app_risks.iter()) {
        if *risk != TargetRisk::Ok {
            disabled_apps.push(app.display_name.clone());
        }
        if app
            .launch_args
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            stripped_args += 1;
        }
    }

    let mut notes: Vec<String> = Vec::new();
    if !disabled_apps.is_empty() {
        notes.push(format!(
            "{} 个应用的路径不存在或位于网络位置，已停用：{}",
            disabled_apps.len(),
            disabled_apps.join("、")
        ));
    }
    if stripped_args > 0 {
        notes.push(format!(
            "{stripped_args} 个应用的启动参数已在导入时剥离，需要时请手动重新填写"
        ));
    }
    let description = match notes.is_empty() {
        true => parsed
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        false => {
            let base = parsed
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| format!("{s}\n\n"))
                .unwrap_or_default();
            Some(format!("{base}[导入提示] {}", notes.join("；")))
        }
    };

    sqlx::query(
        "INSERT INTO workspaces (id, name, description, is_favorite, sort_order,
            auto_snapshot_enabled, created_at, updated_at)
         VALUES (?, ?, ?, 0, 0, ?, ?, ?)",
    )
    .bind(&ws_id)
    .bind(name)
    .bind(description.as_deref())
    .bind(if parsed.auto_snapshot_enabled { 1 } else { 0 })
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let mut app_ids: Vec<String> = Vec::with_capacity(parsed.apps.len());
    let mut seen_app_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut app_count = 0i64;

    for (idx, app) in parsed.apps.iter().enumerate() {
        let display = app.display_name.trim();
        let display = if display.is_empty() { "未命名应用" } else { display };
        let identity = identity_of(&app.target, display);
        if !seen_app_keys.insert(identity.clone()) {
            // 重复应用项合并映射到已创建项
            let existing_pos = parsed
                .apps
                .iter()
                .take(idx)
                .position(|a| {
                    let d = a.display_name.trim();
                    let d = if d.is_empty() { "未命名应用" } else { d };
                    identity_of(&a.target, d) == identity
                })
                .unwrap_or(0);
            app_ids.push(app_ids[existing_pos].clone());
            continue;
        }

        let item_id = format!(
            "it-{:x}-{}-a{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        );
        let policy = match app.reuse_policy.as_str() {
            "reuse_if_matched" | "always_launch" | "reuse_or_launch" => app.reuse_policy.as_str(),
            _ => "reuse_or_launch",
        };

        sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, 'application', ?, ?, ?, 'path', ?, ?, ?, ?)",
        )
        .bind(&item_id)
        .bind(&ws_id)
        .bind(display)
        .bind(&identity)
        .bind(&app.target)
        // 路径不可信的应用一律停用：用户确认后再手动启用
        .bind(if app.enabled && app_risks[idx] == TargetRisk::Ok {
            1
        } else {
            0
        })
        .bind(app_count)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO app_configs (item_id, reuse_policy, launch_args_json, wait_timeout_ms)
             VALUES (?, ?, NULL, 8000)",
        )
        .bind(&item_id)
        .bind(policy)
        .execute(&mut *tx)
        .await?;

        app_ids.push(item_id);
        app_count += 1;
    }

    let mut res_count = 0i64;
    let mut seen_res: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for (idx, r) in parsed.resources.iter().enumerate() {
        if !matches!(r.item_type.as_str(), "file" | "folder" | "url") {
            return Err(AppError::msg(format!("导入文件包含不支持的资源类型：{}", r.item_type)));
        }
        let target = r.target.trim();
        if target.is_empty() {
            continue;
        }
        if r.item_type == "url"
            && !(target.starts_with("http://") || target.starts_with("https://"))
        {
            return Err(AppError::msg(format!("导入文件中的网址不合法：{target}")));
        }
        let key = (r.item_type.clone(), target.to_lowercase());
        if !seen_res.insert(key) {
            continue;
        }

        let item_id = format!(
            "it-{:x}-{}-r{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        );
        let display = if r.display_name.trim().is_empty() {
            default_display_name(&r.item_type, target)
        } else {
            r.display_name.trim().to_string()
        };

        sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item_id)
        .bind(&ws_id)
        .bind(&r.item_type)
        .bind(&display)
        .bind(target.to_lowercase())
        .bind(target)
        .bind(&r.item_type)
        .bind(if r.enabled { 1 } else { 0 })
        .bind(app_count + res_count)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        res_count += 1;
    }

    let mut win_count = 0i64;
    for (idx, w) in parsed.windows.iter().enumerate() {
        let Some(item_id) = app_ids.get(w.app_index) else {
            continue;
        };
        let (xr, yr, wr, hr) = sanitize_layout(&LayoutUpdate {
            window_id: String::new(),
            x_ratio: w.x_ratio,
            y_ratio: w.y_ratio,
            width_ratio: w.width_ratio,
            height_ratio: w.height_ratio,
            monitor_key: None,
            show_state: None,
        });
        let win_id = format!(
            "wl-{:x}-{}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        );
        sqlx::query(
            "INSERT INTO window_layouts (id, workspace_id, item_id, monitor_key,
                x_ratio, y_ratio, width_ratio, height_ratio, source_dpi,
                show_state, z_order, title, class_name, captured_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&win_id)
        .bind(&ws_id)
        .bind(item_id)
        .bind(&w.monitor_key)
        .bind(xr)
        .bind(yr)
        .bind(wr)
        .bind(hr)
        .bind(w.source_dpi)
        .bind(state_to_str(w.show_state))
        .bind(w.z_order)
        .bind(&w.title)
        .bind(&w.class_name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        win_count += 1;
    }

    tx.commit().await?;

    let app_paths: Vec<String> = parsed
        .apps
        .iter()
        .filter(|a| a.enabled)
        .filter_map(|a| a.target.clone())
        .filter(|p| !p.is_empty())
        .take(6)
        .collect();

    Ok(WorkspaceSummary {
        id: ws_id,
        name: name.to_string(),
        description: parsed.description,
        color: None,
        is_favorite: false,
        item_count: app_count,
        resource_count: res_count,
        window_count: win_count,
        created_at: now,
        updated_at: now,
        last_restored_at: None,
        app_paths,
        hotkey_slot: None,
    })
}

/// 重新捕获当前桌面并更新该工作空间的应用清单与窗口布局：
/// 1. 覆盖前先为旧配置创建受保护快照（UI 规范 §16.3）；
/// 2. 保留已有资源（文件/文件夹/网址）及同名应用的复用策略与启动参数；
/// 3. 写入新窗口布局并追加一份手动快照。
pub async fn recapture(
    db: &Db,
    workspace_id: &str,
    capture: &CaptureResult,
) -> AppResult<WorkspaceDetail> {
    if capture.windows.is_empty() {
        return Err(AppError::msg("当前没有可捕获的桌面窗口，未修改工作空间"));
    }

    let prev = get(db, workspace_id).await?;

    // 保留已有应用的复用策略与启动参数（按 identity_key 匹配）
    let mut prev_configs: std::collections::HashMap<String, (String, Option<String>, bool)> =
        std::collections::HashMap::new();
    for it in prev.items.iter().filter(|i| i.item_type == "application") {
        let key = identity_of(&it.target, &it.display_name);
        prev_configs.insert(
            key,
            (it.reuse_policy.clone(), it.launch_args.clone(), it.enabled),
        );
    }

    // 覆盖前先生成一份受保护快照，避免配置丢失
    if let Ok(prev_scene) = to_scene(db, workspace_id).await {
        if let Ok(saved) = storage::save_scene_file("重新捕获前保护", prev_scene) {
            let _ = sqlx::query(
                "INSERT INTO snapshots (id, workspace_id, name, snapshot_type, trigger_type,
                    captured_at, window_count, display_count, payload_path,
                    payload_version, is_protected)
                 VALUES (?, ?, ?, 'protection', 'recapture', ?, ?, ?, ?, 1, 1)",
            )
            .bind(&saved.id)
            .bind(workspace_id)
            .bind(&saved.name)
            .bind(saved.captured_at)
            .bind(saved.window_count as i64)
            .bind(saved.display_count as i64)
            .bind(&saved.path)
            .execute(&db.pool)
            .await;
        }
    }

    let now = now_ms();
    let mut tx = db.pool.begin().await?;

    // 仅清理应用项与窗口布局，保留用户手动维护的资源（file / folder / url）
    sqlx::query("DELETE FROM window_layouts WHERE workspace_id = ?")
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM workspace_items WHERE workspace_id = ? AND item_type = 'application'",
    )
    .bind(workspace_id)
    .execute(&mut *tx)
    .await?;

    let mut item_ids: Vec<(String, String)> = Vec::new();
    for w in &capture.windows {
        let identity = identity_of(&w.process_path, &w.process_name);
        if item_ids.iter().any(|(k, _)| *k == identity) {
            continue;
        }
        let item_id = format!(
            "it-{:x}-{}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            item_ids.len()
        );
        let display_name = if w.process_name.is_empty() {
            w.title.clone()
        } else {
            w.process_name.clone()
        };
        let (policy, args, enabled) = prev_configs
            .get(&identity)
            .cloned()
            .unwrap_or_else(|| ("reuse_or_launch".to_string(), None, true));

        sqlx::query(
            "INSERT INTO workspace_items (id, workspace_id, item_type, display_name,
                identity_key, target, target_kind, enabled, sort_order, created_at, updated_at)
             VALUES (?, ?, 'application', ?, ?, ?, 'path', ?, ?, ?, ?)",
        )
        .bind(&item_id)
        .bind(workspace_id)
        .bind(&display_name)
        .bind(&identity)
        .bind(&w.process_path)
        .bind(if enabled { 1 } else { 0 })
        .bind(item_ids.len() as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO app_configs (item_id, reuse_policy, launch_args_json, wait_timeout_ms)
             VALUES (?, ?, ?, 8000)",
        )
        .bind(&item_id)
        .bind(&policy)
        .bind(&args)
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
        .bind(format!(
            "wl-{:x}-{}-{}",
            now,
            WS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            idx
        ))
        .bind(workspace_id)
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

    sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let _ = add_snapshot(
        db,
        workspace_id,
        "重新捕获更新",
        "manual",
        "recapture",
        capture,
        true,
    )
    .await;

    get(db, workspace_id).await
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
    async fn hotkey_slot_is_stable_and_exclusive() {
        let db = db().await;
        let a = create_from_capture(&db, "写作", &capture()).await.unwrap();
        let b = create_from_capture(&db, "开发", &capture()).await.unwrap();

        // 绑定后按槽能查回来
        set_hotkey_slot(&db, &a.id, Some(2)).await.unwrap();
        assert_eq!(id_for_slot(&db, 2).await.as_deref(), Some(a.id.as_str()));

        // 槽位独占：b 抢走 2 之后，a 必须被解绑（否则唯一索引会炸）
        set_hotkey_slot(&db, &b.id, Some(2)).await.unwrap();
        assert_eq!(id_for_slot(&db, 2).await.as_deref(), Some(b.id.as_str()));

        let rows = list(&db).await.unwrap();
        let slot_of = |id: &str| rows.iter().find(|w| w.id == id).unwrap().hotkey_slot;
        assert_eq!(slot_of(&a.id), None, "被抢占的空间应回到未绑定");
        assert_eq!(slot_of(&b.id), Some(2));

        // 列表顺序变化（改名会改 updated_at 排序）不能影响槽位含义
        rename(&db, &a.id, "写作（改）").await.unwrap();
        let rows = list(&db).await.unwrap();
        assert_eq!(
            rows.iter()
                .find(|w| w.hotkey_slot == Some(2))
                .map(|w| w.id.clone()),
            Some(b.id.clone()),
            "槽位必须跟着工作空间走，而不是跟着列表位置"
        );

        // 解绑
        set_hotkey_slot(&db, &b.id, None).await.unwrap();
        assert_eq!(id_for_slot(&db, 2).await, None);

        // 删除后槽位自然释放
        set_hotkey_slot(&db, &a.id, Some(1)).await.unwrap();
        delete(&db, &a.id).await.unwrap();
        assert_eq!(id_for_slot(&db, 1).await, None);
    }

    #[tokio::test]
    async fn hotkey_slot_rejects_out_of_range() {
        let db = db().await;
        let a = create_from_capture(&db, "边界", &capture()).await.unwrap();

        for bad in [0i64, -1, 10, 99] {
            let err = set_hotkey_slot(&db, &a.id, Some(bad)).await.unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("快捷槽"),
                "越界槽位应给出可读错误，实际：{msg}"
            );
        }
        // 1 和 9 是合法边界
        set_hotkey_slot(&db, &a.id, Some(1)).await.unwrap();
        set_hotkey_slot(&db, &a.id, Some(9)).await.unwrap();
        assert_eq!(id_for_slot(&db, 9).await.as_deref(), Some(a.id.as_str()));
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
            monitor_key: None,
            show_state: None,
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
            monitor_key: None,
            show_state: None,
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
                monitor_key: Some("M2".into()),
                show_state: Some(ShowState::Maximized),
            }],
        )
        .await
        .unwrap();

        let after = get(&db, &s.id).await.unwrap();
        let updated = after.windows.iter().find(|w| w.id == first.id).unwrap();
        assert!((updated.x_ratio - 0.25).abs() < 1e-9);
        assert_eq!(updated.monitor_key, "M2");
        assert_eq!(updated.show_state, ShowState::Maximized);

        let err = update_layouts(
            &db,
            &s.id,
            &[LayoutUpdate {
                window_id: "not-mine".into(),
                x_ratio: 0.1,
                y_ratio: 0.1,
                width_ratio: 0.2,
                height_ratio: 0.2,
                monitor_key: None,
                show_state: None,
            }],
        )
        .await;
        assert!(err.is_err(), "不属于该工作空间的布局必须被拒绝");
    }

    #[tokio::test]
    async fn disabled_application_is_excluded_from_scene() {
        let db = db().await;
        let s = create_from_capture(&db, "禁用应用", &capture()).await.unwrap();
        let detail = get(&db, &s.id).await.unwrap();
        let first_app = detail
            .items
            .iter()
            .find(|i| i.item_type == "application")
            .unwrap();
        let owned = detail
            .windows
            .iter()
            .filter(|w| w.item_id == first_app.id)
            .count();

        update_item(&db, &first_app.id, "reuse_or_launch", None, false, None, None)
            .await
            .unwrap();

        let scene = to_scene(&db, &s.id).await.unwrap();
        assert_eq!(scene.windows.len(), detail.windows.len() - owned);
    }

    #[tokio::test]
    async fn favorite_meta_duplicate_and_export_import_roundtrip() {
        let db = db().await;
        let s = create_from_capture(&db, "原始空间", &capture()).await.unwrap();
        add_item(&db, &s.id, "url", "https://example.com", Some("示例"))
            .await
            .unwrap();

        set_favorite(&db, &s.id, true).await.unwrap();
        update_meta(
            &db,
            &s.id,
            "开发主空间",
            Some("主力开发环境"),
            false,
            Some("#0071e3"),
        )
        .await
        .unwrap();

        let detail = get(&db, &s.id).await.unwrap();
        assert!(detail.is_favorite);
        assert_eq!(detail.name, "开发主空间");
        assert_eq!(detail.description.as_deref(), Some("主力开发环境"));
        assert_eq!(detail.color.as_deref(), Some("#0071e3"));
        assert!(!detail.auto_snapshot_enabled);

        let batch = add_urls_batch(
            &db,
            &s.id,
            &[
                ("https://example.com".into(), Some("重复".into())),
                ("github.com".into(), Some("GitHub".into())),
            ],
        )
        .await
        .unwrap();
        assert_eq!(batch, 1);

        let dup = duplicate(&db, &s.id, "开发备份").await.unwrap();
        assert_ne!(dup.id, s.id);
        assert_eq!(dup.name, "开发备份");
        assert_eq!(dup.item_count, 2);
        assert_eq!(dup.resource_count, 2);
        assert_eq!(dup.window_count, 3);

        let exported = export_json(&db, &s.id).await.unwrap();
        let imported = import_json(&db, &exported).await.unwrap();
        assert_ne!(imported.id, s.id);
        assert_eq!(imported.item_count, 2);
        assert_eq!(imported.resource_count, 2);
        assert_eq!(imported.window_count, 3);

        assert!(import_json(&db, "{bad json").await.is_err());
    }

    #[tokio::test]
    async fn recapture_preserves_resources_and_creates_protection_snapshot() {
        let db = db().await;
        let s = create_from_capture(&db, "重捕测试", &capture()).await.unwrap();
        add_item(&db, &s.id, "folder", r"D:\Work", None).await.unwrap();

        let mut next_cap = capture();
        next_cap.windows.truncate(1);
        let updated = recapture(&db, &s.id, &next_cap).await.unwrap();

        assert_eq!(updated.windows.len(), 1);
        assert!(
            updated.items.iter().any(|i| i.item_type == "folder"),
            "重新捕获必须保留用户添加的资源"
        );
        let snaps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM snapshots WHERE workspace_id = ? AND snapshot_type = 'protection'",
        )
        .bind(&s.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(snaps >= 1, "覆盖前必须自动留存保护快照");
    }

    #[tokio::test]
    async fn relocate_item_updates_path_and_rejects_missing_file() {
        let db = db().await;
        let s = create_from_capture(&db, "重定位", &capture()).await.unwrap();
        let detail = get(&db, &s.id).await.unwrap();
        let app = detail
            .items
            .iter()
            .find(|i| i.item_type == "application")
            .unwrap();

        // 不存在的文件必须拒绝
        assert!(relocate_item(&db, &s.id, &app.id, r"Z:\nonexistent\foo.exe")
            .await
            .is_err());

        // 用真实存在的当前测试可执行文件验证重定位
        let exe = std::env::current_exe().unwrap();
        let exe_str = exe.to_string_lossy().to_string();
        let updated = relocate_item(&db, &s.id, &app.id, &exe_str).await.unwrap();
        assert_eq!(updated.target.as_deref(), Some(exe_str.as_str()));
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
    async fn import_disables_untrusted_apps_and_strips_launch_args() {
        let db = db().await;
        let real_exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let json = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 0,
            "name": "外部导入",
            "description": "来自同事",
            "autoSnapshotEnabled": true,
            "apps": [
                { "displayName": "可信应用", "target": real_exe, "enabled": true,
                  "reusePolicy": "reuse_or_launch", "launchArgs": "--danger" },
                { "displayName": "缺失应用", "target": "Z:\\nowhere\\missing.exe", "enabled": true },
                { "displayName": "网络应用", "target": "\\\\server\\share\\tool.exe", "enabled": true }
            ],
            "resources": [],
            "windows": []
        })
        .to_string();

        let summary = import_json(&db, &json).await.unwrap();
        let detail = get(&db, &summary.id).await.unwrap();

        let by_name = |n: &str| detail.items.iter().find(|i| i.display_name == n).unwrap();

        // 真实存在的 exe 保持启用，但启动参数必须被剥离
        assert!(by_name("可信应用").enabled, "存在的 exe 不应被停用");
        assert!(
            by_name("可信应用").launch_args.is_none(),
            "导入时启动参数必须被剥离"
        );

        // 不存在 / UNC 路径的应用一律停用
        assert!(!by_name("缺失应用").enabled);
        assert!(!by_name("网络应用").enabled);

        // 风险要写进描述，界面上看得见
        let desc = detail.description.unwrap_or_default();
        assert!(desc.contains("[导入提示]"), "描述里应有导入提示：{desc}");
        assert!(desc.contains("2 个应用的路径"), "应说明停用数量：{desc}");
        assert!(desc.contains("启动参数已在导入时剥离"), "应说明参数被剥离：{desc}");
        assert!(desc.starts_with("来自同事"), "不应丢掉原描述：{desc}");
    }

    #[tokio::test]
    async fn import_preview_lists_risky_apps_without_writing() {
        let real_exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let json = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 0,
            "name": "预览",
            "apps": [
                { "displayName": "好应用", "target": real_exe },
                { "displayName": "坏应用", "target": "Q:\\no.exe", "launchArgs": "-x" }
            ],
            "resources": [],
            "windows": []
        })
        .to_string();

        let preview = import_preview(&json).unwrap();
        assert_eq!(preview.app_count, 2);
        assert_eq!(preview.disabled_apps, vec!["坏应用".to_string()]);
        assert_eq!(preview.apps_with_args, vec!["坏应用".to_string()]);
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
