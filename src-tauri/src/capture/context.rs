//! L2 上下文自动捕获（100% 本地只读，无网络、无隐私读取）：
//! 1. VS Code / Cursor / Windsurf / Trae / Antigravity：从本机 globalStorage/storage.json
//!    解析已知工作区路径，按当前打开的 IDE 窗口标题精确匹配真实项目目录；
//! 2. Windows 文件资源管理器（CabinetWClass）：通过 COM Shell.Application 读取当前打开的
//!    文件夹路径（CREATE_NO_WINDOW 静默执行）。

use crate::core::models::WindowInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedContext {
    /// "folder"（资源管理器目录）或 "ide_project"（IDE 项目目录）
    pub kind: String,
    /// 来源应用名（如 explorer.exe / Code.exe / Cursor.exe）
    pub source_app: String,
    /// 展示名（文件夹基名）
    pub display_name: String,
    /// 真实本地路径
    pub target: String,
}

/// 扫描当前捕获窗口对应的 L2 上下文（IDE 项目目录 + 资源管理器已打开文件夹）
pub fn detect_from_windows(windows: &[WindowInfo]) -> Vec<DetectedContext> {
    let mut out: Vec<DetectedContext> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. IDE 项目目录识别（纯本地 JSON 读取，< 2ms）
    let mut ide_cache: HashMap<&'static str, Vec<String>> = HashMap::new();
    for w in windows {
        let exe = w.process_name.to_lowercase();
        let Some(appdata_dir) = ide_appdata_folder(&exe) else {
            continue;
        };
        let known_dirs = ide_cache
            .entry(appdata_dir)
            .or_insert_with(|| load_vscode_family_workspaces(appdata_dir));
        if let Some(matched) = match_title_to_workspace(&w.title, known_dirs) {
            let key = format!("ide:{}", matched.to_lowercase());
            if seen.insert(key) {
                let name = Path::new(&matched)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&matched)
                    .to_string();
                out.push(DetectedContext {
                    kind: "ide_project".into(),
                    source_app: w.process_name.clone(),
                    display_name: name,
                    target: matched,
                });
            }
        }
    }

    // 2. 资源管理器打开的文件夹识别（仅在当前桌面确实有资源管理器窗口时执行）
    let has_explorer = windows.iter().any(|w| {
        w.class_name.eq_ignore_ascii_case("CabinetWClass")
            || w.process_name.eq_ignore_ascii_case("explorer.exe")
    });
    if has_explorer {
        for dir in query_explorer_folders() {
            let key = format!("folder:{}", dir.to_lowercase());
            if seen.insert(key) {
                let name = Path::new(&dir)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&dir)
                    .to_string();
                out.push(DetectedContext {
                    kind: "folder".into(),
                    source_app: "explorer.exe".into(),
                    display_name: name,
                    target: dir,
                });
            }
        }
    }

    out
}

fn ide_appdata_folder(exe_lower: &str) -> Option<&'static str> {
    match exe_lower {
        "code.exe" => Some("Code"),
        "code - insiders.exe" => Some("Code - Insiders"),
        "cursor.exe" => Some("Cursor"),
        "windsurf.exe" => Some("Windsurf"),
        "trae.exe" => Some("Trae"),
        "antigravity.exe" => Some("Antigravity"),
        _ => None,
    }
}

/// 从 VS Code 家族的 globalStorage/storage.json 提取本地已有项目目录
fn load_vscode_family_workspaces(folder: &str) -> Vec<String> {
    let Ok(appdata) = std::env::var("APPDATA") else {
        return Vec::new();
    };
    let storage_path = Path::new(&appdata)
        .join(folder)
        .join("User")
        .join("globalStorage")
        .join("storage.json");
    let Ok(text) = std::fs::read_to_string(storage_path) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };

    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push_uri = |uri: &str| {
        if let Some(p) = file_uri_to_windows_path(uri) {
            if Path::new(&p).is_dir() && seen.insert(p.to_lowercase()) {
                paths.push(p);
            }
        }
    };

    if let Some(obj) = val
        .get("profileAssociations")
        .and_then(|v| v.get("workspaces"))
        .and_then(|v| v.as_object())
    {
        for k in obj.keys() {
            push_uri(k);
        }
    }
    if let Some(arr) = val
        .get("windowsState")
        .and_then(|v| v.get("openedWindows"))
        .and_then(|v| v.as_array())
    {
        for win in arr {
            if let Some(f) = win.get("folder").and_then(|v| v.as_str()) {
                push_uri(f);
            }
        }
    }
    if let Some(f) = val
        .get("windowsState")
        .and_then(|v| v.get("lastActiveWindow"))
        .and_then(|v| v.get("folder"))
        .and_then(|v| v.as_str())
    {
        push_uri(f);
    }

    paths
}

/// 将 `file:///d%3A/AI_Work/WorkScene` 转为 `D:\AI_Work\WorkScene`
pub fn file_uri_to_windows_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file:///")?;
    let decoded = percent_decode(rest);
    let win_path = decoded.replace('/', "\\");
    if win_path.len() >= 2 && win_path.as_bytes()[1] == b':' {
        let mut chars = win_path.chars();
        let drive = chars.next()?.to_ascii_uppercase();
        Some(format!("{drive}{}", chars.as_str()))
    } else {
        None
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h1), Some(h2)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h1 << 4) | h2);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 按 IDE 窗口标题分隔符（` - ` / ` — ` / ` – `）精确匹配目录基名，支持含连字符的项目名（如 my-app）
pub fn match_title_to_workspace(title: &str, workspaces: &[String]) -> Option<String> {
    let normalized = title.replace(" — ", " - ").replace(" – ", " - ");
    let segments: Vec<&str> = normalized
        .split(" - ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    for dir in workspaces {
        let Some(base) = Path::new(dir).file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if segments.iter().any(|seg| seg.eq_ignore_ascii_case(base)) {
            return Some(dir.clone());
        }
    }
    None
}

/// 静默查询当前已打开的 Windows 文件资源管理器文件夹路径（带 3 秒超时与 UNC 跳过，永不阻塞）
fn query_explorer_folders() -> Vec<String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script = "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; (New-Object -ComObject Shell.Application).Windows() | ForEach-Object { try { $p=$_.Document.Folder.Self.Path; if ($p -and -not $p.StartsWith('\\\\')) { Write-Output $p } } catch {} }";
    let Ok(mut child) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    else {
        return Vec::new();
    };

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Vec::new();
                }
                let mut stdout = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout);
                }
                return stdout
                    .lines()
                    .map(|l| l.trim().trim_start_matches('\u{feff}'))
                    .filter(|l| !l.is_empty() && !l.starts_with(r"\\") && Path::new(l).is_dir())
                    .map(|l| l.to_string())
                    .collect();
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Vec::new();
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {
                let _ = child.kill();
                return Vec::new();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_vscode_file_uri_with_spaces_and_chinese() {
        assert_eq!(
            file_uri_to_windows_path("file:///d%3A/AI_Work/WorkScene"),
            Some(r"D:\AI_Work\WorkScene".to_string())
        );
        assert_eq!(
            file_uri_to_windows_path("file:///c%3A/My%20Projects/app"),
            Some(r"C:\My Projects\app".to_string())
        );
    }

    #[test]
    fn matches_ide_window_title_segment_to_workspace_dir() {
        let dirs = vec![
            r"D:\AI_Work\WorkScene".to_string(),
            r"D:\AI_Work\OtherApp".to_string(),
        ];
        assert_eq!(
            match_title_to_workspace("Restore.tsx - WorkScene - Visual Studio Code", &dirs),
            Some(r"D:\AI_Work\WorkScene".to_string())
        );
        assert_eq!(
            match_title_to_workspace("Welcome - Visual Studio Code", &dirs),
            None
        );
    }
}
