use crate::capture;
use crate::core::models::{
    AppInfo, CaptureResult, DisplayInfo, RestorePreview, RestoreReport, SceneFile, SceneSummary,
};
use crate::core::{AppError, AppResult};
use crate::storage;

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
pub fn scene_list() -> AppResult<Vec<SceneSummary>> {
    storage::list_scenes()
}

#[tauri::command]
pub fn scene_save(name: String, capture: CaptureResult) -> AppResult<SceneSummary> {
    if capture.windows.is_empty() {
        return Err(AppError::msg("当前没有可保存的窗口，请先打开需要记录的应用"));
    }
    storage::save_scene(&name, &capture)
}

#[tauri::command]
pub fn scene_get(id: String) -> AppResult<SceneFile> {
    storage::load_scene(&id)
}

#[tauri::command]
pub fn scene_delete(id: String) -> AppResult<()> {
    storage::delete_scene(&id)
}

/// 损坏快照诊断：列表页据此提示「N 个快照损坏」，而不是让文件静默消失
#[tauri::command]
pub fn scene_health() -> AppResult<storage::SceneHealth> {
    storage::scene_health()
}

#[tauri::command]
pub fn run_list(limit: Option<usize>) -> AppResult<Vec<RestoreReport>> {
    storage::list_runs(limit.unwrap_or(20))
}

#[tauri::command]
pub fn restore_preview(id: String) -> AppResult<RestorePreview> {
    let scene = storage::load_scene(&id)?;
    crate::restore::preview(&scene)
}

/// 恢复是阻塞型任务（需要等待窗口稳定），放进阻塞线程池，避免卡住 UI。
#[tauri::command]
pub async fn restore_apply(
    app: tauri::AppHandle,
    id: String,
    relaunch_missing: bool,
) -> AppResult<RestoreReport> {
    let scene = storage::load_scene(&id)?;

    // 恢复前自动保护快照：批量移动窗口是本产品风险最高的动作，
    // 必须先留一份现场，用户才能一键撤销（评审补充意见 A8）
    let pre_restore = capture::scan()
        .and_then(|capture| storage::save_scene("恢复前自动保护", &capture))
        .map(|summary| summary.id)
        .ok();

    let handle = tauri::async_runtime::spawn_blocking(move || {
        crate::restore::apply(Some(&app), &scene, relaunch_missing, pre_restore)
    })
    .await;

    match handle {
        Ok(result) => result,
        Err(e) => Err(AppError::msg(format!("恢复任务异常终止：{e}"))),
    }
}
