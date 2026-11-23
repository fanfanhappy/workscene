//! Chrome/Edge Native Messaging host.
//!
//! 同一个 WorkScene.exe 既可以是 GUI，也可以由浏览器以 native host 身份启动。
//! 浏览器启动时没有参数，因此用“父进程是 chrome/msedge 且 stdin 是管道”来识别。
//! host 只把扩展发来的标签页快照写到本机 data 目录，不访问网络。

use crate::core::time::now_ms;
use serde_json::{json, Value};
use std::io::{Read, Write};

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_WINDOWS: usize = 20;
const MAX_TABS_PER_WINDOW: usize = 120;
const MAX_TITLE_CHARS: usize = 300;
const MAX_URL_CHARS: usize = 4096;

pub fn is_native_messaging_host() -> bool {
    // Native Messaging 的 stdin 一定是管道；普通 GUI 从浏览器下载后启动时
    // stdin 不是管道，即使父进程恰好是 chrome.exe 也不能误入 host 模式。
    if !stdin_is_pipe() {
        return false;
    }
    parent_process_name()
        .map(|name| is_browser_process(&name))
        .unwrap_or(false)
}

#[cfg(windows)]
fn stdin_is_pipe() -> bool {
    use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_PIPE};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE};
    unsafe { GetFileType(GetStdHandle(STD_INPUT_HANDLE)) == FILE_TYPE_PIPE }
}

#[cfg(not(windows))]
fn stdin_is_pipe() -> bool {
    false
}

fn is_browser_process(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "chrome.exe"
            | "msedge.exe"
            | "brave.exe"
            | "vivaldi.exe"
            | "chromium.exe"
            | "arc.exe"
            | "opera.exe"
    )
}

#[cfg(windows)]
fn parent_process_name() -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let me = std::process::id();
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut parent_id = 0u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == me {
                    parent_id = entry.th32ParentProcessID;
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }

        let mut parent_name = None;
        if parent_id != 0 {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    if entry.th32ProcessID == parent_id {
                        let end = entry
                            .szExeFile
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.szExeFile.len());
                        parent_name = Some(String::from_utf16_lossy(&entry.szExeFile[..end]));
                        break;
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
        }
        let _ = CloseHandle(snap);
        parent_name
    }
}

#[cfg(not(windows))]
fn parent_process_name() -> Option<String> {
    None
}

fn write_snapshot(payload: &Value) -> Result<usize, String> {
    let browser = payload
        .get("browser")
        .and_then(Value::as_str)
        .filter(|s| *s == "chrome" || *s == "edge")
        .unwrap_or("chrome");
    let mut windows = Vec::new();
    if let Some(list) = payload.get("windows").and_then(Value::as_array) {
        for window in list.iter().take(MAX_WINDOWS) {
            let mut tabs = Vec::new();
            if let Some(tab_list) = window.get("tabs").and_then(Value::as_array) {
                for tab in tab_list.iter().take(MAX_TABS_PER_WINDOW) {
                    let url = tab.get("url").and_then(Value::as_str).unwrap_or("");
                    if !(url.starts_with("http://")
                        || url.starts_with("https://")
                        || url.starts_with("file://"))
                    {
                        continue;
                    }
                    let title = tab
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .chars()
                        .take(MAX_TITLE_CHARS)
                        .collect::<String>();
                    tabs.push(json!({
                        "title": title,
                        "url": url.chars().take(MAX_URL_CHARS).collect::<String>(),
                        "active": tab.get("active").and_then(Value::as_bool).unwrap_or(false),
                    }));
                }
            }
            if tabs.is_empty() {
                continue;
            }
            windows.push(json!({
                "id": window.get("id").and_then(Value::as_i64).unwrap_or(0),
                "focused": window.get("focused").and_then(Value::as_bool).unwrap_or(false),
                "state": window.get("state").and_then(Value::as_str).unwrap_or("normal"),
                "tabs": tabs,
            }));
        }
    }

    let snapshot = json!({
        "version": 1,
        "browser": browser,
        "receivedAt": now_ms(),
        "windows": windows,
    });
    let dir = crate::storage::data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
    let path = dir.join(format!("browser-tabs-{browser}.json"));
    let tmp = dir.join(format!("browser-tabs-{browser}.json.tmp"));
    let body = serde_json::to_vec(&snapshot).map_err(|e| format!("序列化标签页失败：{e}"))?;
    std::fs::write(&tmp, body).map_err(|e| format!("写入标签页快照失败：{e}"))?;
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换标签页快照失败：{e}"))?;
    Ok(snapshot["windows"]
        .as_array()
        .map(|w| {
            w.iter()
                .map(|x| x["tabs"].as_array().map(Vec::len).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0))
}

fn read_message<R: Read>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "native message too large",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)?;
    writer.write_all(&(body.len() as u32).to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

/// 处理一条 native message 后退出。
/// 扩展使用 `sendNativeMessage`，Chrome 读取响应后会关闭 host；这里不能继续等第二条，
/// 否则可能和浏览器互相等待。
pub fn run() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    let message = match read_message(&mut input) {
        Ok(Some(m)) => m,
        Ok(None) => return,
        Err(e) => {
            let _ = write_message(&mut output, &json!({ "ok": false, "error": e.to_string() }));
            return;
        }
    };
    let response = match serde_json::from_slice::<Value>(&message) {
        Ok(payload) => match write_snapshot(&payload) {
            Ok(tabs) => json!({ "ok": true, "tabs": tabs }),
            Err(e) => json!({ "ok": false, "error": e }),
        },
        Err(e) => json!({ "ok": false, "error": format!("invalid json: {e}") }),
    };
    let _ = write_message(&mut output, &response);
}
