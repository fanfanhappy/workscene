//! 会话标记：判断上次是否"正常退出"，为崩溃后的恢复提示提供依据。
//!
//! 做法：启动时读取旧标记；若旧标记存在且 `clean == false`，说明上次没走正常退出流程。
//! 随后写入本次标记（clean=false），退出时改写为 clean=true 或直接删除。

use crate::core::time::now_ms;
use crate::core::AppResult;
use crate::storage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMarker {
    pub pid: u32,
    pub started_at: i64,
    pub clean: bool,
}

fn marker_path() -> PathBuf {
    storage::data_dir().join("session.json")
}

fn read_marker() -> Option<SessionMarker> {
    let text = std::fs::read_to_string(marker_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_marker(marker: &SessionMarker) {
    if let Ok(json) = serde_json::to_string_pretty(marker) {
        let _ = std::fs::write(marker_path(), json);
    }
}

/// 启动时调用：返回「上次未正常退出」的标记（若有），并写入本次会话标记。
pub fn begin() -> Option<SessionMarker> {
    let _ = std::fs::create_dir_all(storage::data_dir());
    let previous = read_marker().filter(|m| !m.clean);

    write_marker(&SessionMarker {
        pid: std::process::id(),
        started_at: now_ms(),
        clean: false,
    });

    previous
}

/// 正常退出时调用
pub fn mark_clean() {
    if let Some(mut m) = read_marker() {
        m.clean = true;
        write_marker(&m);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryHint {
    pub crashed_at: i64,
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub snapshot_id: Option<String>,
    pub snapshot_at: Option<i64>,
}

/// 组装恢复提示：崩溃标记 + 激活工作空间 + 该空间最近一份快照
pub fn write_hint(hint: &RecoveryHint) -> AppResult<()> {
    let path = storage::data_dir().join("recovery-hint.json");
    let json = serde_json::to_string_pretty(hint)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn read_hint() -> Option<RecoveryHint> {
    let path = storage::data_dir().join("recovery-hint.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn clear_hint() {
    let _ = std::fs::remove_file(storage::data_dir().join("recovery-hint.json"));
}
