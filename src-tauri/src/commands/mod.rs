use crate::capture;
use crate::core::models::{
    AppInfo, CaptureResult, DisplayInfo, RestorePreview, RestoreReport, SceneFile,
};
use crate::core::{AppError, AppResult};
use crate::data::runs::{self, RunSummary};
use crate::data::session;
use crate::data::settings;
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

async fn scan_blocking() -> AppResult<CaptureResult> {
    tauri::async_runtime::spawn_blocking(capture::scan)
        .await
        .map_err(|e| AppError::msg(format!("扫描桌面任务异常：{e}")))?
}

#[tauri::command]
pub fn system_get_displays() -> AppResult<Vec<DisplayInfo>> {
    capture::displays()
}

#[tauri::command]
pub async fn capture_start() -> AppResult<CaptureResult> {
    scan_blocking().await
}

#[tauri::command]
pub async fn scene_health() -> AppResult<storage::SceneHealth> {
    tauri::async_runtime::spawn_blocking(storage::scene_health)
        .await
        .map_err(|e| AppError::msg(format!("快照健康检查异常：{e}")))?
}

/// 同步 Windows 原生窗口标题栏深浅色，避免浅色主题顶着黑色标题栏
#[tauri::command]
pub fn window_set_theme(app: tauri::AppHandle, theme: String) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        let target = match theme.as_str() {
            "light" => Some(tauri::Theme::Light),
            "dark" => Some(tauri::Theme::Dark),
            _ => None,
        };
        let _ = win.set_theme(target);
    }
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
    app: tauri::AppHandle,
    db: State<'_, Db>,
    name: String,
    capture: CaptureResult,
) -> AppResult<WorkspaceSummary> {
    let ws = workspaces::create_from_capture(&db, &name, &capture).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(ws)
}

#[tauri::command]
pub async fn workspace_rename(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    name: String,
) -> AppResult<()> {
    workspaces::rename(&db, &id, &name).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn workspace_set_favorite(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    is_favorite: bool,
) -> AppResult<()> {
    workspaces::set_favorite(&db, &id, is_favorite).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn workspace_update_meta(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    name: String,
    description: Option<String>,
    auto_snapshot_enabled: bool,
    color: Option<String>,
) -> AppResult<()> {
    workspaces::update_meta(
        &db,
        &id,
        &name,
        description.as_deref(),
        auto_snapshot_enabled,
        color.as_deref(),
    )
    .await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn workspace_set_hotkey_slot(
    db: State<'_, Db>,
    id: String,
    slot: Option<i64>,
) -> AppResult<()> {
    workspaces::set_hotkey_slot(&db, &id, slot).await
}

#[tauri::command]
pub async fn workspace_duplicate(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    new_name: String,
) -> AppResult<WorkspaceSummary> {
    let ws = workspaces::duplicate(&db, &id, &new_name).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(ws)
}

#[tauri::command]
pub async fn workspace_export(db: State<'_, Db>, id: String) -> AppResult<String> {
    workspaces::export_json(&db, &id).await
}

#[tauri::command]
pub async fn workspace_import(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    json: String,
) -> AppResult<WorkspaceSummary> {
    let ws = workspaces::import_json(&db, &json).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(ws)
}

#[tauri::command]
pub async fn workspace_recapture(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
) -> AppResult<WorkspaceDetail> {
    let capture = scan_blocking().await?;
    let detail = workspaces::recapture(&db, &id, &capture).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(detail)
}

/// 用用户在捕获页勾选的窗口子集更新已有工作空间（保留原资源与同名应用启动参数，覆盖前自动备份）
#[tauri::command]
pub async fn workspace_update_from_capture(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    capture: CaptureResult,
) -> AppResult<WorkspaceDetail> {
    let detail = workspaces::recapture(&db, &id, &capture).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(detail)
}

#[tauri::command]
pub async fn workspace_delete(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
) -> AppResult<()> {
    workspaces::delete(&db, &id).await?;
    crate::win::tray::refresh_menu(&app).await;
    Ok(())
}

/// 重新定位应用可执行路径
#[tauri::command]
pub async fn workspace_item_relocate(
    db: State<'_, Db>,
    workspace_id: String,
    item_id: String,
    new_path: String,
) -> AppResult<workspaces::WorkspaceItem> {
    workspaces::relocate_item(&db, &workspace_id, &item_id, &new_path).await
}

/// 更新单个应用项：复用策略 / 启动参数 / 启用状态 / 启动顺序 / 启动延迟
#[tauri::command]
pub async fn workspace_item_update(
    db: State<'_, Db>,
    item_id: String,
    reuse_policy: String,
    launch_args: Option<String>,
    enabled: bool,
    launch_priority: Option<i64>,
    launch_delay_ms: Option<i64>,
) -> AppResult<()> {
    workspaces::update_item(
        &db,
        &item_id,
        &reuse_policy,
        launch_args,
        enabled,
        launch_priority,
        launch_delay_ms,
    )
    .await
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

/// 添加资源项：文件 / 文件夹 / 网址
#[tauri::command]
pub async fn workspace_item_add(
    db: State<'_, Db>,
    workspace_id: String,
    item_type: String,
    target: String,
    display_name: Option<String>,
) -> AppResult<workspaces::WorkspaceItem> {
    workspaces::add_item(&db, &workspace_id, &item_type, &target, display_name.as_deref()).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlBatchEntry {
    pub url: String,
    pub name: Option<String>,
}

/// 批量添加网址资源（多行粘贴或导入浏览器书签夹）
#[tauri::command]
pub async fn workspace_items_add_batch(
    db: State<'_, Db>,
    workspace_id: String,
    entries: Vec<UrlBatchEntry>,
) -> AppResult<usize> {
    let pairs: Vec<(String, Option<String>)> = entries
        .into_iter()
        .map(|e| (e.url, e.name))
        .collect();
    workspaces::add_urls_batch(&db, &workspace_id, &pairs).await
}

/// 批量提取本地 .exe 软件真实图标（Data URL，内存缓存）
#[tauri::command]
pub async fn app_icons_batch(
    paths: Vec<String>,
) -> AppResult<std::collections::HashMap<String, String>> {
    tauri::async_runtime::spawn_blocking(move || crate::win::icon::extract_batch(&paths))
        .await
        .map_err(|e| AppError::msg(format!("提取图标异常：{e}")))
}

/// 只读扫描本机 Chrome / Edge 书签文件夹（不读密码、Cookie 或历史记录）
#[tauri::command]
pub async fn bookmarks_list_folders() -> AppResult<Vec<crate::win::bookmarks::BookmarkFolder>> {
    tauri::async_runtime::spawn_blocking(crate::win::bookmarks::list_folders)
        .await
        .map_err(|e| AppError::msg(format!("读取书签目录异常：{e}")))
}

/// 一键收工：向当前打开且属于该工作空间的窗口发送标准 WM_CLOSE（绝不强杀进程）
#[tauri::command]
pub async fn workspace_close_windows(db: State<'_, Db>, id: String) -> AppResult<usize> {
    let scene = workspaces::to_scene(&db, &id).await?;
    tauri::async_runtime::spawn_blocking(move || crate::restore::close_workspace_windows(&scene))
        .await
        .map_err(|e| AppError::msg(format!("关闭窗口异常：{e}")))?
}

/// 导入前预检：只看不写，供 UI 提示"这份文件会启动什么"
#[tauri::command]
pub fn workspace_import_preview(
    raw_json: String,
) -> AppResult<workspaces::ImportPreview> {
    workspaces::import_preview(&raw_json)
}

#[tauri::command]
pub async fn workspace_preview(db: State<'_, Db>, id: String) -> AppResult<RestorePreview> {
    let scene = workspaces::to_scene(&db, &id).await?;
    tauri::async_runtime::spawn_blocking(move || crate::restore::preview(&scene))
        .await
        .map_err(|e| AppError::msg(format!("生成恢复预览异常：{e}")))?
}

/// 从工作空间恢复。流程与顺序即 PRD §3.3：
/// 读配置 → 生成计划 → 恢复前保护快照 → 执行 → 归档报告。
#[tauri::command]
pub async fn workspace_restore(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    relaunch_missing: bool,
    focus_mode: Option<bool>,
) -> AppResult<RestoreReport> {
    // 同一时刻只允许一个恢复任务：两个恢复互相搬窗口会让报告完全失真
    let _guard = crate::restore::try_acquire()
        .ok_or_else(|| AppError::msg("已有恢复任务在执行，请等它结束再试"))?;

    let scene = workspaces::to_scene(&db, &id).await?;
    let snapshot_id = workspaces::latest_snapshot(&db, &id).await?;
    let focus = focus_mode.unwrap_or(false);

    let pre_restore = match scan_blocking().await {
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
        crate::restore::apply_with_options(
            Some(&app),
            &scene_for_task,
            relaunch_missing,
            focus,
            pre_for_task,
            None,
        )
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

    let pre_restore = match (&workspace_id, scan_blocking().await) {
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

/// 单次恢复的逐步结果（关窗/重启后仍可回查）
#[tauri::command]
pub async fn run_steps(
    db: State<'_, Db>,
    run_id: String,
) -> AppResult<Vec<runs::RunStepRow>> {
    runs::steps(&db, &run_id).await
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

/// 请求取消正在执行的恢复任务（不强制关闭已启动的应用）
#[tauri::command]
pub fn restore_cancel() -> bool {
    crate::restore::request_cancel()
}

/// 读取单份快照的完整布局与窗口内容，用于历史页可视化预览
#[tauri::command]
pub async fn snapshot_get(id: String) -> AppResult<SceneFile> {
    tauri::async_runtime::spawn_blocking(move || storage::load_scene(&id))
        .await
        .map_err(|e| AppError::msg(format!("读取快照异常：{e}")))?
}

/// 仅重试上次恢复中失败或跳过的项，不重复移动已经恢复成功的窗口
#[tauri::command]
pub async fn workspace_restore_retry(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: String,
    relaunch_missing: bool,
    target_names: Vec<String>,
) -> AppResult<RestoreReport> {
    let _guard = crate::restore::try_acquire()
        .ok_or_else(|| AppError::msg("已有恢复任务在执行，请等它结束再试"))?;

    let targets: std::collections::HashSet<String> = target_names.into_iter().collect();
    if targets.is_empty() {
        return Err(AppError::msg("没有需要重试的项目"));
    }

    let scene = workspaces::to_scene(&db, &id).await?;
    let snapshot_id = workspaces::latest_snapshot(&db, &id).await?;

    let scene_for_task = scene.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        crate::restore::apply_subset(Some(&app), &scene_for_task, relaunch_missing, None, Some(&targets))
    })
    .await;

    let report = match handle {
        Ok(Ok(report)) => report,
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(AppError::msg(format!("重试任务异常终止：{e}"))),
    };

    let _ = runs::save(&db, &report, Some(&id), snapshot_id.as_deref(), "retry").await;
    Ok(report)
}

/// 弹出 Windows 原生文件选择器（`exe_only = true` 仅筛选程序）
#[tauri::command]
pub async fn dialog_pick_file(exe_only: Option<bool>) -> AppResult<Option<String>> {
    let only_exe = exe_only.unwrap_or(false);
    let title = if only_exe {
        "选择应用程序 (.exe)"
    } else {
        "选择要添加的文件"
    };
    let path = tauri::async_runtime::spawn_blocking(move || {
        crate::win::dialog::pick_file(title, only_exe)
    })
    .await
    .map_err(|e| AppError::msg(format!("文件选择框异常：{e}")))?;
    Ok(path)
}

/// 弹出 Windows 原生文件夹选择器
#[tauri::command]
pub async fn dialog_pick_folder() -> AppResult<Option<String>> {
    let path = tauri::async_runtime::spawn_blocking(|| {
        crate::win::dialog::pick_folder("选择要添加的文件夹")
    })
    .await
    .map_err(|e| AppError::msg(format!("文件夹选择框异常：{e}")))?;
    Ok(path)
}

/// 在资源管理器中打开本机数据目录（仅固定打开自身数据目录，不接受外部路径参数）
#[tauri::command]
pub fn system_open_data_dir() -> AppResult<()> {
    storage::ensure_dirs()?;
    let dir = storage::data_dir();
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|e| AppError::msg(format!("无法打开数据目录：{e}")))?;
    Ok(())
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
    let capture = scan_blocking().await?;
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
    let _ = snapshots::get(&db, &id_a).await?;
    let _ = snapshots::get(&db, &id_b).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let a = storage::load_scene(&id_a)?;
        let b = storage::load_scene(&id_b)?;
        Ok(snapshots::diff(&a, &b))
    })
    .await
    .map_err(|e| AppError::msg(format!("快照对比异常：{e}")))?
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

/// 批量识别软件的适配等级（供设置页「我常用软件的适配状态」一次返回）
#[tauri::command]
pub fn adapter_classify_batch(
    exe_names: Vec<String>,
) -> std::collections::HashMap<String, String> {
    exe_names
        .into_iter()
        .map(|name| {
            let kind = crate::adapters::classify(&name).as_str().to_string();
            (name, kind)
        })
        .collect()
}

// ---- 设置与存储（M8） ----

#[tauri::command]
pub async fn settings_get(db: State<'_, Db>) -> AppResult<settings::AppSettings> {
    let mut s = settings::get(&db).await?;
    // 开机启动的真源是注册表，不用数据库里的值覆盖
    s.autostart = crate::win::autostart::is_enabled();
    Ok(s)
}

#[tauri::command]
pub async fn settings_set(
    db: State<'_, Db>,
    settings: settings::AppSettings,
) -> AppResult<settings::AppSettings> {
    // 先应用注册表，成功后才落库，避免界面显示"已开启"但实际没生效
    let autostart_ok = crate::win::autostart::sync(settings.autostart);
    let saved = settings::set(&db, &settings).await?;
    crate::win::filter::set_ignored_processes(&saved.ignored_processes);

    // 快捷键：热键注册是线程绑定的，这里只把"换成哪个"投递给热键线程
    let combo = crate::win::hotkey::parse_preset(&saved.hotkey);
    let hotkey_ok = crate::win::hotkey::update(combo, saved.workspace_hotkeys_enabled);

    if !autostart_ok {
        return Err(AppError::msg(
            "开机启动设置写入注册表失败（可能被组策略限制）",
        ));
    }
    if !hotkey_ok && crate::win::hotkey::parse_preset(&saved.hotkey).is_some() {
        return Err(AppError::msg(
            "快捷键注册失败（可能被其它程序占用），已保存设置但本次未生效",
        ));
    }
    Ok(saved)
}

#[tauri::command]
pub async fn storage_stats(db: State<'_, Db>) -> AppResult<settings::StorageStats> {
    settings::storage_stats(&db).await
}

/// 按当前保留策略清理所有工作空间的旧自动快照
#[tauri::command]
pub async fn storage_cleanup(db: State<'_, Db>) -> AppResult<usize> {
    settings::prune_all(&db).await
}
