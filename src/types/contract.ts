/**
 * IPC 契约：与 src-tauri/src/core/models.rs 一一对应。
 * 任何字段改动必须两侧同步，Rust 侧 serde 使用 camelCase。
 */

export type ShowState = "normal" | "maximized" | "minimized";

export interface WindowInfo {
  hwnd: number;
  processId: number;
  processPath: string | null;
  processName: string;
  processStartedAt: number | null;
  title: string;
  className: string;
  x: number;
  y: number;
  width: number;
  height: number;
  dpi: number;
  monitorKey: string | null;
  isVisible: boolean;
  isMinimized: boolean;
  isMaximized: boolean;
  showState: ShowState;
}

export interface DisplayInfo {
  monitorKey: string;
  deviceName: string;
  x: number;
  y: number;
  width: number;
  height: number;
  workX: number;
  workY: number;
  workWidth: number;
  workHeight: number;
  dpi: number;
  scaleFactor: number;
  isPrimary: boolean;
}

/** 被过滤掉的窗口，保留用于诊断（文档 §8.3 要求保留排除原因） */
export interface ExcludedWindow {
  title: string;
  className: string;
  processName: string;
  reason: string;
}

export interface CaptureResult {
  capturedAt: number;
  durationMs: number;
  windows: WindowInfo[];
  displays: DisplayInfo[];
  excluded: ExcludedWindow[];
  diagnostics: string[];
}

export interface SceneWindow {
  processPath: string | null;
  processName: string;
  className: string;
  title: string;
  /** 相对目标显示器工作区的比例坐标，跨显示器分辨率仍然可用 */
  monitorKey: string;
  xRatio: number;
  yRatio: number;
  widthRatio: number;
  heightRatio: number;
  /** 捕获时的原始物理像素，仅用于诊断与比较 */
  originalX: number;
  originalY: number;
  originalWidth: number;
  originalHeight: number;
  sourceDpi: number;
  showState: ShowState;
  zOrder: number;
}

export interface SceneResource {
  itemType: string;
  target: string;
  displayName: string;
}

export interface SceneFile {
  schemaVersion: number;
  id: string;
  name: string;
  capturedAt: number;
  displays: DisplayInfo[];
  windows: SceneWindow[];
  resources?: SceneResource[];
}

export interface SceneSummary {
  id: string;
  name: string;
  capturedAt: number;
  windowCount: number;
  displayCount: number;
  path: string;
}

export type StepStatus =
  | "success"
  | "failed"
  | "skipped"
  | "timeout"
  | "needsReview";

export interface PreviewItem {
  targetName: string;
  detail: string;
  status: StepStatus;
  /** 匹配策略说明，让用户知道「凭什么被识别」 */
  matchNote: string;
}

export interface RestorePreview {
  sceneId: string;
  sceneName: string;
  items: PreviewItem[];
  displayNotes: string[];
  summary: { total: number; ready: number; warning: number; missing: number };
}

export interface RestoreStepResult {
  targetName: string;
  status: StepStatus;
  message: string;
  matchNote: string;
  durationMs: number;
  deltaX: number | null;
  deltaY: number | null;
/** 显示状态（最大化/最小化）是否与快照一致 */
  stateRestored: boolean;
  /** 机器可读错误码，见 docs/RESTORE_CONTRACT.md */
  code: string | null;
}

export interface RestoreReport {
  runId: string;
  sceneId: string;
  sceneName: string;
  startedAt: number;
  finishedAt: number;
  totalSteps: number;
  succeeded: number;
  failed: number;
  skipped: number;
  steps: RestoreStepResult[];
  displayNotes: string[];
  /** 恢复前自动创建的保护快照，撤销时重新 apply 它 */
  preRestoreSceneId: string | null;
}

export interface CorruptedScene {
  file: string;
  reason: string;
}

export interface SceneHealth {
  corrupted: CorruptedScene[];
}

// ---- M2：工作空间（SQLite） ----

export interface WorkspaceSummary {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  isFavorite: boolean;
  itemCount: number;
  resourceCount: number;
  windowCount: number;
  createdAt: number;
  updatedAt: number;
  lastRestoredAt: number | null;
  appPaths: string[];
}

export interface WorkspaceItem {
  id: string;
  itemType: string;
  displayName: string;
  target: string | null;
  targetKind: string;
  enabled: boolean;
  reusePolicy: string;
  launchArgs: string | null;
  launchPriority: number;
  launchDelayMs: number;
}

export interface BookmarkUrl {
  name: string;
  url: string;
}

export interface BookmarkFolder {
  browser: string;
  folderPath: string;
  urls: BookmarkUrl[];
}

export interface UrlBatchEntry {
  url: string;
  name?: string | null;
}

export interface WorkspaceWindow {
  id: string;
  itemId: string;
  title: string;
  className: string;
  processPath: string | null;
  processName: string;
  monitorKey: string;
  xRatio: number;
  yRatio: number;
  widthRatio: number;
  heightRatio: number;
  sourceDpi: number | null;
  showState: ShowState;
  zOrder: number;
}

export interface WorkspaceDetail {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  icon: string | null;
  isFavorite: boolean;
  autoSnapshotEnabled: boolean;
  createdAt: number;
  updatedAt: number;
  lastRestoredAt: number | null;
  items: WorkspaceItem[];
  windows: WorkspaceWindow[];
}

/** 布局编辑：一次提交多个窗口的新比例坐标、可选目标显示器与显示状态 */
export interface LayoutUpdate {
  windowId: string;
  xRatio: number;
  yRatio: number;
  widthRatio: number;
  heightRatio: number;
  monitorKey?: string;
  showState?: ShowState;
}

// ---- M5：Hybrid 快照 ----

export interface SnapshotSummary {
  id: string;
  workspaceId: string;
  name: string;
  snapshotType: string;
  triggerType: string;
  capturedAt: number;
  windowCount: number;
  displayCount: number;
  isProtected: boolean;
  fingerprint: string | null;
}

export interface WindowDiff {
  change: "added" | "removed" | "moved" | "state_changed";
  titleA: string;
  titleB: string;
  detail: string;
}

export interface SnapshotDiff {
  added: number;
  removed: number;
  moved: number;
  stateChanged: number;
  unchanged: number;
  windows: WindowDiff[];
  displayChanged: boolean;
  displayDetail: string;
}

// ---- M6：崩溃恢复提示 / 适配建议 ----

export interface RecoveryHint {
  crashedAt: number;
  workspaceId: string | null;
  workspaceName: string | null;
  snapshotId: string | null;
  snapshotAt: number | null;
}

export interface AdapterSuggestion {
  kind: string;
  args: string | null;
}

// ---- M8：设置与存储 ----

export interface AppSettings {
  autoSnapshotEnabled: boolean;
  autoKeep: number;
  autoMinIntervalSecs: number;
  restoreRelaunchDefault: boolean;
  /** 恢复时默认开启专注模式（自动最小化非本空间窗口，不关闭软件） */
  restoreFocusModeDefault: boolean;
  /** 开机启动：真源是注册表 HKCU Run */
  autostart: boolean;
  /** 全局快捷键预设，空串表示关闭 */
  hotkey: string;
  /** 是否启用 Ctrl+Alt+1~5 快速切换前 5 个工作空间 */
  workspaceHotkeysEnabled: boolean;
  /** 启动 WorkScene 时自动恢复的工作空间 ID */
  startupWorkspaceId: string | null;
  /** 全局捕获忽略名单（进程名，扫描桌面时自动跳过） */
  ignoredProcesses: string[];
}

export interface StorageStats {
  dataDir: string;
  dbBytes: number;
  snapshotCount: number;
  snapshotBytes: number;
  runCount: number;
}

export interface RunStepRow {
  seq: number;
  stepType: string;
  targetName: string;
  status: string;
  errorCode: string | null;
  message: string;
  matchNote: string | null;
  durationMs: number | null;
  deltaX: number | null;
  deltaY: number | null;
  stateRestored: boolean;
}

export interface RunSummary {  id: string;
  workspaceId: string | null;
  snapshotId: string | null;
  status: string;
  triggerType: string;
  startedAt: number;
  finishedAt: number | null;
  totalSteps: number;
  succeeded: number;
  failed: number;
  skipped: number;
}

export interface AppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  dpiAwareness: string;
  dataDir: string;
  snapshotDir: string;
}
