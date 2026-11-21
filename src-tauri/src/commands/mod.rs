use crate::capture;
use crate::core::models::{
    AppInfo, CaptureResult, DisplayInfo, RestorePreview, RestoreReport, SceneFile,
};
use crate::core::{AppError, AppResult};
use crate::data::runs::{self, RunSummary};
use crate::data::session;
use crate::data::snapshots::{self, SnapshotDiff, SnapshotSummary};
use crate::data::workspaces::{self, WorkspaceDetail, WorkspaceSummary};
use crate::data::Db;
use crate::storage;
use tauri::State;

#[tauri::command]
pub fn system_get_info() -> AppInfo {
    AppInfo {
        name: "WorkScene".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        tauri_version: tauri::VERSION.into(),
        dpi_awareness: "Per-Monitor V2".into(),
        data_dir: storage::data_dir().to_string_lossy().to_string(),
        snapshot_dir: storage::snapshot_dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn system_get_displays() -> AppResult<Vec<DisplayInfo>> {
    let capture = capture::scan()?;
    Ok(capture.displays)
}

#[tauri::command]
pub fn capture_start() -> AppResult<CaptureResult> {
    capture::scan()
}

#[tauri::command]
pub fn scene_health() -> AppResult<storage::SceneHealth> {
    storage::scene_health()
}

#[tauri::command]
pub async fn workspace_list(db: State<'_, Db>) -> AppResult<Vec<WorkspaceSummary>> {
    workspaces::list(&db).await
}

#[tauri::command]
pub async fn workspace_get(db: State<'_, Db>, id: String) -> AppResult<WorkspaceDetail> {
    workspaces::get(&db, &id).await
}

#[tauri::command]
pub async fn workspace_create(
    db: State<'_, Db>,
    name: String,
    capture: CaptureResult,
) -> AppResult<WorkspaceSummary> {
    workspaces::create_from_capture(&db, &name, &capture).await
}

#[tauri::command]
pub async fn workspace_rename(db: State<'_, Db>, id: String, name: String) -> AppResult<()> {
    workspaces::rename(&db, &id, &name).await
}

#[tauri::command]
pub async fn workspace_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    workspaces::delete(&db, &id).await
}

/// 更新单个应用项：复用策略 / 启动参数 / 启用状态
#[tauri::command]
pub async fn workspace_item_update(
    db: State<'_, Db>,
    item_id: String,
    reuse_policy: String,
    launch_args: Option<String>,
    enabled: bool,
) -> AppResult<()> {
    workspaces::update_item(&db, &item_id, &reuse_policy, launch_args, enabled).await
}

/// 布局编辑：批量提交窗口的新比例坐标（一个事务）
#[tauri::command]
pub async fn workspace_layout_update(
    db: State<'_, Db>,
    workspace_id: String,
    updates: Vec<workspaces::LayoutUpdate>,
) -> AppResult<()> {
    workspaces::update_layouts(&db, &workspace_id, &updates).await
}

#[tauri::command]
pub async fn workspace_window_delete(
    db: State<'_, Db>,
    workspace_id: String,
    window_id: String,
) -> AppResult<()> {
    workspaces::delete_window(&db, &workspace_id, &window_id).await
}

#[tauri::command]
pub async fn workspace_item_delete(
    db: State<'_, Db>,
    workspace_id: String,
    item_id: String,
) -> AppResult<()> {
    workspaces::delete_item(&db, &workspace_id, &item_id).await
}

#[tauri::command]
pub async fn workspace_preview(db: State<'_, Db>, id: String) -> AppResult<RestorePreview> {
    let scene = workspaces::to_scene(&db, &id).await?;
    crate::restore::preview(&scene)
}

/// 从工作空间恢复。流程与顺序即 PRD §3.3：
/// 读配置 → 生成计划 → 恢复前保护快照 → 执行 → 归档报告。
#[tauri::command]
pub async fn workspace_restore(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    relaunch_missing: bool,
) -> AppResult<RestoreReport> {
    // 同一时刻只允许一个恢复任务：两个恢复互相搬窗口会让报告完全失真
    let _guard = crate::restore::try_acquire()
        .ok_or_else(|| AppError::msg("已有恢复任务在执行，请等它结束再试"))?;

    let scene = workspaces::to_scene(&db, &id).await?;
    let snapshot_id = workspaces::latest_snapshot(&db, &id).await?;

    let pre_restore = match capture::scan() {
        Ok(current) => workspaces::add_snapshot(
            &db,
            &id,
            "恢复前自动保护",
            "protection",
            "restore",
            &current,
            true,
        )
        .await
        .ok(),
        Err(_) => None,
    };

    let scene_for_task = scene.clone();
    let pre_for_task = pre_restore.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        crate::restore::apply(Some(&app), &scene_for_task, relaunch_missing, pre_for_task)
    })
    .await;

    let report = match handle {
        Ok(Ok(report)) => report,
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(AppError::msg(format!("恢复任务异常终止：{e}"))),
    };

    let _ = runs::save(&db, &report, Some(&id), snapshot_id.as_deref(), "manual").await;
    let _ = workspaces::mark_restored(&db, &id).await;
    // 恢复过的空间成为"当前激活空间"，自动快照据此归属
    let _ = snapshots::set_active_workspace(&db, &id).await;

    Ok(report)
}

/// 按快照 payload 恢复（「撤销这次恢复」与历史回退都走这里）
#[tauri::command]
pub async fn snapshot_restore(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    relaunch_missing: bool,
) -> AppResult<RestoreReport> {
    let _guard = crate::restore::try_acquire()
        .ok_or_else(|| AppError::msg("已有恢复任务在执行，请等它结束再试"))?;

    let scene: SceneFile = storage::load_scene(&id)?;
    let workspace_id = workspaces::workspace_of_snapshot(&db, &id).await?;

    let pre_restore = match (&workspace_id, capture::scan()) {
        (Some(ws), Ok(current)) => workspaces::add_snapshot(
            &db,
            ws,
            "恢复前自动保护",
            "protection",
            "restore",
            &current,
            true,
        )
        .await
        .ok(),
        _ => None,
    };

    let scene_for_task = scene.clone();
    let pre_for_task = pre_restore.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        crate::restore::apply(Some(&app), &scene_for_task, relaunch_missing, pre_for_task)
    })
    .await;

    let report = match handle {
        Ok(Ok(report)) => report,
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(AppError::msg(format!("恢复任务异常终止：{e}"))),
    };

    if let Some(ws) = &workspace_id {
        let _ = runs::save(&db, &report, Some(ws), Some(&id), "snapshot").await;
        let _ = workspaces::mark_restored(&db, ws).await;
    } else {
        let _ = runs::save(&db, &report, None, Some(&id), "snapshot").await;
    }

    Ok(report)
}

#[tauri::command]
pub async fn run_list(db: State<'_, Db>, limit: Option<usize>) -> AppResult<Vec<RunSummary>> {
    runs::list(&db, limit.unwrap_or(20)).await
}

/// 成功率基线：按 error_code 聚合。没有这一层，R8 的数字只能靠"跑一遍看看"。
#[tauri::command]
pub async fn run_error_stats(db: State<'_, Db>) -> AppResult<Vec<(String, i64)>> {
    runs::error_code_stats(&db).await
}

/// 是否已有恢复任务在执行（UI 据此禁用"开始恢复"并显示原因）
#[tauri::command]
pub fn restore_is_busy() -> bool {
    crate::restore::is_busy()
}

// ---- 快照（M5：Hybrid） ----

#[tauri::command]
pub async fn snapshot_list(
    db: State<'_, Db>,
    workspace_id: String,
    limit: Option<usize>,
) -> AppResult<Vec<SnapshotSummary>> {
    snapshots::list(&db, &workspace_id, limit.unwrap_or(50)).await
}

/// 手动快照：立即捕获当前桌面并归档（永远创建，不做去重）
#[tauri::command]
pub async fn snapshot_create(db: State<'_, Db>, workspace_id: String, name: String) -> AppResult<SnapshotSummary> {
    let capture = capture::scan()?;
    let name = if name.trim().is_empty() {
        "手动快照".to_string()
    } else {
        name
    };
    snapshots::create_manual(&db, &workspace_id, &name, &capture).await
}

#[tauri::command]
pub async fn snapshot_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    snapshots::delete(&db, &id).await
}

#[tauri::command]
pub async fn snapshot_diff(
    db: State<'_, Db>,
    id_a: String,
    id_b: String,
) -> AppResult<SnapshotDiff> {
    let a = storage::load_scene(&id_a)?;
    let b = storage::load_scene(&id_b)?;
    let _ = snapshots::get(&db, &id_a).await?;
    let _ = snapshots::get(&db, &id_b).await?;
    Ok(snapshots::diff(&a, &b))
}

#[tauri::command]
pub async fn snapshot_prune(
    db: State<'_, Db>,
    workspace_id: String,
    keep: usize,
) -> AppResult<usize> {
    snapshots::prune(&db, &workspace_id, keep).await
}

// ---- 崩溃恢复提示（M6） ----

/// 存在即表示"上次 WorkScene 没有正常退出"
#[tauri::command]
pub fn recovery_hint() -> Option<session::RecoveryHint> {
    session::read_hint()
}

#[tauri::command]
pub fn recovery_dismiss() {
    session::clear_hint();
}

// ---- 应用适配建议（L2，仅建议不自动填） ----

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterSuggestion {
    pub kind: String,
    pub args: Option<String>,
}

#[tauri::command]
pub fn adapter_suggest(exe_name: String, target: Option<String>) -> AdapterSuggestion {
    let kind = crate::adapters::classify(&exe_name);
    AdapterSuggestion {
        kind: kind.as_str().to_string(),
        args: crate::adapters::suggest_args(&exe_name, target.as_deref()),
    }
}
