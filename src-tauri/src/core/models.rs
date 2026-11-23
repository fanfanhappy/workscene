use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShowState {
    Normal,
    Maximized,
    Minimized,
}

impl Default for ShowState {
    fn default() -> Self {
        ShowState::Normal
    }
}

/// 浏览器标签页（来自可选的浏览器扩展桥；只保存 URL/标题，不保存页面内容）
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub title: String,
    pub url: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    /// 仅当前运行时有效，绝不作为持久标识（文档 §7.2）
    pub hwnd: usize,
    pub process_id: u32,
    pub process_path: Option<String>,
    pub process_name: String,
    pub process_started_at: Option<i64>,
    pub title: String,
    pub class_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
    pub monitor_key: Option<String>,
    pub is_visible: bool,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub show_state: ShowState,
    /// 仅当浏览器扩展桥已安装并同步时才有值
    #[serde(default)]
    pub browser_tabs: Vec<BrowserTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub monitor_key: String,
    pub device_name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: i32,
    pub work_height: i32,
    pub dpi: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedWindow {
    pub title: String,
    pub class_name: String,
    pub process_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub captured_at: i64,
    pub duration_ms: i64,
    pub windows: Vec<WindowInfo>,
    pub displays: Vec<DisplayInfo>,
    pub excluded: Vec<ExcludedWindow>,
    pub diagnostics: Vec<String>,
}

/// 快照中的窗口：比例坐标 + 原始像素，二者都存（文档 §7.3）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneWindow {
    pub process_path: Option<String>,
    pub process_name: String,
    pub class_name: String,
    pub title: String,
    pub monitor_key: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub width_ratio: f64,
    pub height_ratio: f64,
    pub original_x: i32,
    pub original_y: i32,
    pub original_width: i32,
    pub original_height: i32,
    pub source_dpi: u32,
    pub show_state: ShowState,
    pub z_order: i32,
    /// 浏览器标签页；恢复时按窗口一次性带 URL 启动
    #[serde(default)]
    pub browser_tabs: Vec<BrowserTab>,
    /// 复用策略：reuse_if_matched / always_launch / reuse_or_launch（缺省按后者）
    #[serde(default)]
    pub reuse_policy: String,
    /// 启动参数（命令行字符串，按类 shell 规则切分，绝不经过 cmd.exe）
    #[serde(default)]
    pub launch_args: Option<String>,
    /// 启动优先级：数值越小越先启动（如 -10 代理软件优先启动）
    #[serde(default)]
    pub launch_priority: i64,
    /// 启动本程序后的等待延迟（毫秒），用于等代理/服务就绪后再开后续应用
    #[serde(default)]
    pub launch_delay_ms: u64,
}

/// 非应用类资源：文件、文件夹、网址（PRD FR-008 / FR-009）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneResource {
    /// file / folder / url
    pub item_type: String,
    pub target: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneFile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub captured_at: i64,
    pub displays: Vec<DisplayInfo>,
    pub windows: Vec<SceneWindow>,
    /// 旧快照没有这个字段，读取时默认为空
    #[serde(default)]
    pub resources: Vec<SceneResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub id: String,
    pub name: String,
    pub captured_at: i64,
    pub window_count: usize,
    pub display_count: usize,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    Timeout,
    NeedsReview,
}

fn default_step_type() -> String {
    "window".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub target_name: String,
    pub detail: String,
    pub status: StepStatus,
    pub match_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub scene_id: String,
    pub scene_name: String,
    pub items: Vec<PreviewItem>,
    pub display_notes: Vec<String>,
    pub summary: PreviewSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSummary {
    pub total: usize,
    pub ready: usize,
    pub warning: usize,
    pub missing: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreStepResult {
    /// window / resource
    #[serde(default = "default_step_type")]
    pub step_type: String,
    pub target_name: String,
    pub status: StepStatus,
    pub message: String,
    pub match_note: String,
    pub duration_ms: i64,
    pub delta_x: Option<i32>,
    pub delta_y: Option<i32>,
    /// 显示状态（最大化/最小化）是否与快照一致；不一致时 message 会明确说明
    pub state_restored: bool,
    /// 机器可读错误码，见 docs/RESTORE_CONTRACT.md §3；成功时为 None
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub run_id: String,
    pub scene_id: String,
    pub scene_name: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub total_steps: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub steps: Vec<RestoreStepResult>,
    pub display_notes: Vec<String>,
    /// 恢复前自动创建的保护快照，撤销时重新 apply 它即可
    pub pre_restore_scene_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub tauri_version: String,
    pub dpi_awareness: String,
    pub data_dir: String,
    pub snapshot_dir: String,
    pub log_dir: String,
}
