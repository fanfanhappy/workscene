//! v1.1.0 进阶持久化能力：
//! 1. 多屏幕拓扑布局（Display Profiles）：同一工作空间按不同屏幕组合保存独立窗口排布，恢复时自动匹配；
//! 2. 工作空间生命周期钩子（Hooks）：可选配置「恢复前 / 收工后」本地命令；
//! 3. 资源项内联编辑（修改网址/名称/路径）。

use crate::core::models::{SceneFile, ShowState};
use crate::core::time::now_ms;
use crate::core::{AppError, AppResult};
use crate::data::Db;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

pub async fn ensure_schema(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspace_profiles (
            workspace_id TEXT NOT NULL,
            display_signature TEXT NOT NULL,
            label TEXT NOT NULL,
            layouts_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (workspace_id, display_signature),
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS workspace_hooks (
            workspace_id TEXT PRIMARY KEY,
            pre_restore_cmd TEXT,
            post_close_cmd TEXT,
            enabled INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayProfileSummary {
    pub display_signature: String,
    pub label: String,
    pub window_count: usize,
    pub updated_at: i64,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedProfileLayout {
    pub process_name: String,
    pub class_name: String,
    pub title: String,
    pub monitor_key: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
    pub show_state: ShowState,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHooks {
    pub pre_restore_cmd: String,
    pub post_close_cmd: String,
    pub enabled: bool,
}

/// 生成当前显示器拓扑的稳定签名与可读标签（例如 `2@1920x1080+2560x1440`）
pub fn current_display_signature() -> (String, String) {
    let monitors = crate::win::display::enumerate();
    if monitors.is_empty() {
        return ("1@unknown".into(), "默认单屏".into());
    }
    // 只按分辨率排序会让“双屏并排”和“双屏上下叠放”得到同一个签名，
    // 恢复时把布局套到错误的物理拓扑上。把相对位置和 DPI 也纳入签名。
    let mut parts: Vec<String> = monitors
        .iter()
        .map(|m| {
            format!(
                "{}x{}@{},{}@{}dpi",
                m.rect.right - m.rect.left,
                m.rect.bottom - m.rect.top,
                m.rect.left,
                m.rect.top,
                m.dpi
            )
        })
        .collect();
    parts.sort();
    let sig = format!("{}@{}", monitors.len(), parts.join("+"));
    let prefix = match monitors.len() {
        1 => "单屏",
        2 => "双屏",
        3 => "三屏",
        _ => "多屏",
    };
    let label = format!("{} ({})", prefix, parts.join(" + "));
    (sig, label)
}

/// 将工作空间当前布局保存为「当前显示器拓扑」的专属布局方案
pub async fn save_display_profile(db: &Db, workspace_id: &str) -> AppResult<DisplayProfileSummary> {
    let detail = crate::data::workspaces::get(db, workspace_id).await?;
    let (sig, label) = current_display_signature();
    let saved: Vec<SavedProfileLayout> = detail
        .windows
        .iter()
        .map(|w| SavedProfileLayout {
            process_name: w.process_name.clone(),
            class_name: w.class_name.clone(),
            title: w.title.clone(),
            monitor_key: w.monitor_key.clone(),
            x_ratio: w.x_ratio,
            y_ratio: w.y_ratio,
            width_ratio: w.width_ratio,
            height_ratio: w.height_ratio,
            show_state: w.show_state,
        })
        .collect();
    let json = serde_json::to_string(&saved)?;
    let now = now_ms();

    sqlx::query(
        "INSERT INTO workspace_profiles (workspace_id, display_signature, label, layouts_json, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(workspace_id, display_signature)
         DO UPDATE SET label = excluded.label, layouts_json = excluded.layouts_json, updated_at = excluded.updated_at",
    )
    .bind(workspace_id)
    .bind(&sig)
    .bind(&label)
    .bind(&json)
    .bind(now)
    .execute(&db.pool)
    .await?;

    Ok(DisplayProfileSummary {
        display_signature: sig,
        label,
        window_count: saved.len(),
        updated_at: now,
        is_current: true,
    })
}

/// 若当前显示器组合已经保存过专属方案，则把工作空间的当前布局同步进去。
/// 布局保存/重新捕获后必须调用，否则恢复时 `apply_matching_profile` 会继续用旧方案。
pub async fn sync_current_profile(db: &Db, workspace_id: &str) -> AppResult<bool> {
    let (sig, _) = current_display_signature();
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM workspace_profiles WHERE workspace_id = ? AND display_signature = ?",
    )
    .bind(workspace_id)
    .bind(&sig)
    .fetch_optional(&db.pool)
    .await?;
    if exists.is_none() {
        return Ok(false);
    }
    save_display_profile(db, workspace_id).await?;
    Ok(true)
}

pub async fn list_display_profiles(
    db: &Db,
    workspace_id: &str,
) -> AppResult<Vec<DisplayProfileSummary>> {
    let (current_sig, _) = current_display_signature();
    let rows = sqlx::query(
        "SELECT display_signature, label, layouts_json, updated_at
         FROM workspace_profiles WHERE workspace_id = ? ORDER BY updated_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let sig: String = r.get("display_signature");
            let json: String = r.get("layouts_json");
            let count = serde_json::from_str::<Vec<SavedProfileLayout>>(&json)
                .map(|v| v.len())
                .unwrap_or(0);
            DisplayProfileSummary {
                is_current: sig == current_sig,
                display_signature: sig,
                label: r.get("label"),
                window_count: count,
                updated_at: r.get("updated_at"),
            }
        })
        .collect())
}

pub async fn get_display_profile(
    db: &Db,
    workspace_id: &str,
    display_signature: &str,
) -> AppResult<Vec<SavedProfileLayout>> {
    let row = sqlx::query(
        "SELECT layouts_json FROM workspace_profiles WHERE workspace_id = ? AND display_signature = ?",
    )
    .bind(workspace_id)
    .bind(display_signature)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::msg("未找到指定的屏幕布局方案"))?;

    let json: String = row.get("layouts_json");
    let saved: Vec<SavedProfileLayout> = serde_json::from_str(&json)
        .map_err(|e| AppError::msg(format!("屏幕布局方案数据损坏：{e}")))?;
    Ok(saved)
}

pub async fn delete_display_profile(
    db: &Db,
    workspace_id: &str,
    display_signature: &str,
) -> AppResult<()> {
    sqlx::query("DELETE FROM workspace_profiles WHERE workspace_id = ? AND display_signature = ?")
        .bind(workspace_id)
        .bind(display_signature)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// 恢复前检查：若当前屏幕拓扑存在专属布局方案，自动覆盖 scene.windows 的目标坐标与显示状态
pub async fn apply_matching_profile(db: &Db, workspace_id: &str, scene: &mut SceneFile) -> bool {
    let (current_sig, _) = current_display_signature();
    let row = sqlx::query(
        "SELECT layouts_json FROM workspace_profiles WHERE workspace_id = ? AND display_signature = ?",
    )
    .bind(workspace_id)
    .bind(&current_sig)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();

    let Some(r) = row else {
        return false;
    };
    let json: String = r.get("layouts_json");
    let Ok(saved) = serde_json::from_str::<Vec<SavedProfileLayout>>(&json) else {
        return false;
    };

    let mut used = vec![false; saved.len()];
    let mut applied = false;
    for win in &mut scene.windows {
        // 同一应用多窗口时，先按标题精确配对，再退回“进程+类名”的第一个未用项。
        let matches_identity = |s: &SavedProfileLayout| {
            s.process_name.eq_ignore_ascii_case(&win.process_name)
                && (s.class_name.is_empty() || s.class_name == win.class_name)
        };
        let found = saved
            .iter()
            .enumerate()
            .find(|(i, s)| !used[*i] && matches_identity(s) && s.title == win.title)
            .or_else(|| {
                saved
                    .iter()
                    .enumerate()
                    .find(|(i, s)| !used[*i] && matches_identity(s))
            });
        if let Some((i, s)) = found {
            used[i] = true;
            win.monitor_key = s.monitor_key.clone();
            win.x_ratio = s.x_ratio;
            win.y_ratio = s.y_ratio;
            win.width_ratio = s.width_ratio;
            win.height_ratio = s.height_ratio;
            win.show_state = s.show_state;
            applied = true;
        }
    }
    applied
}

pub async fn get_hooks(db: &Db, workspace_id: &str) -> AppResult<WorkspaceHooks> {
    let row = sqlx::query(
        "SELECT pre_restore_cmd, post_close_cmd, enabled FROM workspace_hooks WHERE workspace_id = ?",
    )
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?;

    Ok(match row {
        Some(r) => WorkspaceHooks {
            pre_restore_cmd: r
                .get::<Option<String>, _>("pre_restore_cmd")
                .unwrap_or_default(),
            post_close_cmd: r
                .get::<Option<String>, _>("post_close_cmd")
                .unwrap_or_default(),
            enabled: r.get::<i64, _>("enabled") != 0,
        },
        None => WorkspaceHooks::default(),
    })
}

pub async fn set_hooks(db: &Db, workspace_id: &str, hooks: &WorkspaceHooks) -> AppResult<()> {
    let now = now_ms();
    sqlx::query(
        "INSERT INTO workspace_hooks (workspace_id, pre_restore_cmd, post_close_cmd, enabled, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(workspace_id)
         DO UPDATE SET pre_restore_cmd = excluded.pre_restore_cmd,
                       post_close_cmd = excluded.post_close_cmd,
                       enabled = excluded.enabled,
                       updated_at = excluded.updated_at",
    )
    .bind(workspace_id)
    .bind(hooks.pre_restore_cmd.trim())
    .bind(hooks.post_close_cmd.trim())
    .bind(if hooks.enabled { 1 } else { 0 })
    .bind(now)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// 执行用户显式开启并配置的本地钩子命令：
/// 等待命令完成（带超时上限，避免挂起恢复流程），返回人类可读的结果或错误说明。
pub fn run_hook_command(cmd: &str) -> Result<String, String> {
    run_hook_command_with_timeout(cmd, std::time::Duration::from_secs(8))
}

pub fn run_hook_command_with_timeout(
    cmd: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::time::Instant;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return Ok("空命令已跳过".into());
    }
    let mut child = std::process::Command::new("cmd")
        .args(["/C", trimmed])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("无法启动命令：{e}"))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(format!("命令执行完成：{trimmed}"));
                }
                let mut err_text = String::new();
                if let Some(mut se) = child.stderr.take() {
                    use std::io::Read;
                    let _ = se.read_to_string(&mut err_text);
                }
                let detail = err_text.trim();
                let code = status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "未知".into());
                return Err(if detail.is_empty() {
                    format!("命令退出码 {code}")
                } else {
                    format!(
                        "命令退出码 {code}：{}",
                        detail.chars().take(160).collect::<String>()
                    )
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // 超时不强杀子进程（例如后台常驻服务启动命令），放行后续恢复并明确提示
                    return Ok(format!(
                        "命令已后台启动（超过 {}s 仍在运行，已继续后续步骤）",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            Err(e) => return Err(format!("等待命令结果失败：{e}")),
        }
    }
}

/// 内联编辑已有资源项（文件 / 文件夹 / 网址）的名称或目标地址（含 URL 协议校验与重复检测）
pub async fn update_resource_item(
    db: &Db,
    workspace_id: &str,
    item_id: &str,
    display_name: &str,
    target: &str,
) -> AppResult<()> {
    let clean_target = target.trim();
    if clean_target.is_empty() {
        return Err(AppError::msg("目标路径或网址不能为空"));
    }

    let row = sqlx::query(
        "SELECT item_type FROM workspace_items
         WHERE id = ? AND workspace_id = ? AND item_type IN ('file', 'folder', 'url')",
    )
    .bind(item_id)
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?;
    let Some(r) = row else {
        return Err(AppError::msg("未找到可编辑的资源项"));
    };
    let item_type: String = r.get("item_type");

    let final_target = if item_type == "url" {
        let lower = clean_target.to_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(AppError::msg("网址必须以 http:// 或 https:// 开头"));
        }
        clean_target.to_string()
    } else {
        clean_target.to_string()
    };

    let clean_name = display_name.trim();
    let final_name = if clean_name.is_empty() {
        final_target.as_str()
    } else {
        clean_name
    };
    let now = now_ms();

    let res = sqlx::query(
        "UPDATE workspace_items
         SET display_name = ?, target = ?, identity_key = ?, updated_at = ?
         WHERE id = ? AND workspace_id = ? AND item_type = ?",
    )
    .bind(final_name)
    .bind(&final_target)
    .bind(final_target.to_lowercase())
    .bind(now)
    .bind(item_id)
    .bind(workspace_id)
    .bind(&item_type)
    .execute(&db.pool)
    .await;

    match res {
        Ok(done) if done.rows_affected() > 0 => {
            let _ = sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(workspace_id)
                .execute(&db.pool)
                .await;
            Ok(())
        }
        Ok(_) => Err(AppError::msg("未找到可编辑的资源项")),
        Err(e) if e.to_string().contains("UNIQUE") => {
            Err(AppError::msg("该目标已存在于当前工作空间的资源列表中"))
        }
        Err(e) => Err(e.into()),
    }
}
