//! 可选的浏览器标签页桥接。
//!
//! 浏览器扩展通过 Native Messaging 把当前窗口/标签页快照写到 data 目录；
//! WorkScene 在捕获时读取快照，把 URL 存进工作空间，恢复时按窗口一次性
//! 用 `--new-window url1 url2 ...` 打开，避免标签页被拆成多个窗口。
//!
//! 这是显式可选能力：未安装扩展时完全不影响现有捕获/恢复。

use crate::core::models::BrowserTab;
use crate::core::time::now_ms;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const HOST_NAME: &str = "com.workscene.browser_tabs";
/// manifest.json 里固定 `key` 推导出的扩展 ID，native host 的 allowed_origins 依赖它。
pub const EXTENSION_ID: &str = "pijcngaddijnocfoikelinkigoacigbl";
const MAX_SNAPSHOT_AGE_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBridgeStatus {
    pub installed: bool,
    pub manifest_path: String,
    pub extension_dir: String,
    pub chrome_registered: bool,
    pub edge_registered: bool,
    pub last_sync_at: Option<i64>,
    pub tab_count: usize,
    pub window_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    #[serde(default)]
    browser: String,
    #[serde(default)]
    received_at: i64,
    #[serde(default)]
    windows: Vec<SnapshotWindow>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotWindow {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    tabs: Vec<BrowserTab>,
}

pub fn manifest_path() -> PathBuf {
    crate::storage::data_dir()
        .join("native-messaging")
        .join(format!("{HOST_NAME}.json"))
}

pub fn extension_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    if let Ok(dir) = app.path().resource_dir() {
        for rel in ["browser-extension", "resources/browser-extension"] {
            let candidate = dir.join(rel);
            if candidate.join("manifest.json").is_file() {
                return candidate;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for rel in ["browser-extension", "resources/browser-extension"] {
                let candidate = parent.join(rel);
                if candidate.join("manifest.json").is_file() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/browser-extension")
}

fn root_for_browser(exe_name: &str) -> Option<(&'static str, &'static str)> {
    match exe_name.to_ascii_lowercase().as_str() {
        "msedge.exe" => Some(("edge", r"Software\Microsoft\Edge\NativeMessagingHosts")),
        "chrome.exe" | "chromium.exe" | "brave.exe" | "vivaldi.exe" | "arc.exe" | "opera.exe" => {
            Some(("chrome", r"Software\Google\Chrome\NativeMessagingHosts"))
        }
        _ => None,
    }
}

fn snapshot_path(browser: &str) -> PathBuf {
    crate::storage::data_dir().join(format!("browser-tabs-{browser}.json"))
}

pub fn load_status(app: &tauri::AppHandle) -> BrowserBridgeStatus {
    let manifest = manifest_path();
    let chrome_registered = registry_installed(
        r"Software\Google\Chrome\NativeMessagingHosts",
        HOST_NAME,
    );
    let edge_registered = registry_installed(
        r"Software\Microsoft\Edge\NativeMessagingHosts",
        HOST_NAME,
    );
    let mut last_sync_at = None;
    let mut tab_count = 0;
    let mut window_count = 0;
    for browser in ["chrome", "edge"] {
        if let Ok(text) = std::fs::read_to_string(snapshot_path(browser)) {
            if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&text) {
                if snapshot.received_at > last_sync_at.unwrap_or(0) {
                    last_sync_at = Some(snapshot.received_at);
                }
                window_count += snapshot.windows.len();
                tab_count += snapshot.windows.iter().map(|w| w.tabs.len()).sum::<usize>();
            }
        }
    }
    BrowserBridgeStatus {
        installed: manifest.is_file() && (chrome_registered || edge_registered),
        manifest_path: manifest.to_string_lossy().to_string(),
        extension_dir: extension_dir(app).to_string_lossy().to_string(),
        chrome_registered,
        edge_registered,
        last_sync_at,
        tab_count,
        window_count,
    }
}

/// 安装 native host manifest + 当前用户的 Chrome/Edge 注册表项。
/// 扩展本身仍需用户在 chrome://extensions 里“加载已解压的扩展程序”。
pub fn install_native_host() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败：{e}"))?;
    let manifest = manifest_path();
    if let Some(parent) = manifest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 native host 目录失败：{e}"))?;
    }
    let body = serde_json::json!({
        "name": HOST_NAME,
        "description": "WorkScene browser tabs bridge",
        "path": exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    });
    std::fs::write(
        &manifest,
        serde_json::to_vec_pretty(&body).map_err(|e| format!("序列化 manifest 失败：{e}"))?,
    )
    .map_err(|e| format!("写入 native host manifest 失败：{e}"))?;

    let mut errors = Vec::new();
    for root in [
        r"Software\Google\Chrome\NativeMessagingHosts",
        r"Software\Microsoft\Edge\NativeMessagingHosts",
        r"Software\Chromium\NativeMessagingHosts",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
        r"Software\Vivaldi\NativeMessagingHosts",
    ] {
        if let Err(e) = register_host(root, HOST_NAME, &manifest) {
            errors.push(format!("{root}: {e}"));
        }
    }
    if errors.len() == 5 {
        return Err(format!("注册浏览器 Native Messaging 失败：{}", errors.join("；")));
    }
    Ok(manifest)
}

pub fn install(app: &tauri::AppHandle) -> Result<BrowserBridgeStatus, String> {
    install_native_host()?;
    Ok(load_status(app))
}

pub fn encode_tabs(tabs: &[BrowserTab]) -> Option<String> {
    if tabs.is_empty() {
        return None;
    }
    serde_json::to_string(tabs).ok()
}

pub fn decode_tabs(raw: Option<&str>) -> Vec<BrowserTab> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<BrowserTab>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| valid_url(&t.url))
        .take(120)
        .collect()
}

pub fn valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")
}

/// 一次捕获期间的浏览器快照缓存；同一个扩展窗口只匹配一次。
pub struct BrowserTabs {
    browser: Option<&'static str>,
    snapshot: Option<Snapshot>,
    used: HashSet<i64>,
}

impl BrowserTabs {
    pub fn load(exe_name: &str) -> Self {
        let Some((browser, _)) = root_for_browser(exe_name) else {
            return Self {
                browser: None,
                snapshot: None,
                used: HashSet::new(),
            };
        };
        let snapshot = std::fs::read_to_string(snapshot_path(browser))
            .ok()
            .and_then(|text| serde_json::from_str::<Snapshot>(&text).ok())
            .filter(|s| s.browser.is_empty() || s.browser == browser)
            .filter(|s| now_ms() - s.received_at <= MAX_SNAPSHOT_AGE_MS);
        if let Some(s) = &snapshot {
            tracing::debug!(
                browser,
                windows = s.windows.len(),
                tabs = s.windows.iter().map(|w| w.tabs.len()).sum::<usize>(),
                received_at = s.received_at,
                "browser tab snapshot loaded"
            );
        }
        Self {
            browser: Some(browser),
            snapshot,
            used: HashSet::new(),
        }
    }

    pub fn for_window(&mut self, title: &str) -> Vec<BrowserTab> {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return Vec::new();
        };
        let target = normalize(title);
        let target_count = title_tab_count(title);
        let mut best: Option<(i64, &SnapshotWindow)> = None;
        for window in &snapshot.windows {
            if self.used.contains(&window.id) || window.tabs.is_empty() {
                continue;
            }
            let active = window
                .tabs
                .iter()
                .find(|t| t.active)
                .or_else(|| window.tabs.first())
                .map(|t| normalize(&t.title))
                .unwrap_or_default();
            if active.is_empty() {
                continue;
            }
            let title_hit = target.contains(&active) || active.contains(&target);
            if !title_hit {
                continue;
            }
            let count_hit = target_count
                .map(|n| n == window.tabs.len())
                .unwrap_or(false);
            let score = (if count_hit { 10 } else { 0 })
                + (if window.focused { 2 } else { 0 })
                + active.len() as i64;
            if best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, window));
            }
        }
        if let Some((_, window)) = best {
            self.used.insert(window.id);
            tracing::debug!(
                browser = self.browser.unwrap_or("-"),
                window_id = window.id,
                tabs = window.tabs.len(),
                title,
                "browser window matched to tab snapshot"
            );
            return window.tabs.clone();
        }
        Vec::new()
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || ('\u{4E00}'..='\u{9FFF}').contains(c))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 从窗口标题提取“和另外 N 个页面”里的 N+1；识别不了返回 None。
fn title_tab_count(title: &str) -> Option<usize> {
    if let Some(pos) = title.find("和另外") {
        let rest = &title[pos + "和另外".len()..];
        let digits: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<usize>() {
            return Some(n + 1);
        }
    }
    let lower = title.to_ascii_lowercase();
    for marker in ["and ", "and"] {
        if let Some(pos) = lower.find(marker) {
            let rest = &lower[pos + marker.len()..];
            let digits: String = rest
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<usize>() {
                if rest.contains("page") || rest.contains("tab") {
                    return Some(n + 1);
                }
            }
        }
    }
    None
}

/// 把浏览器标签页转成启动参数。跳过非 http(s)/file 页面，并限制总长度。
pub fn urls_for_launch(tabs: &[BrowserTab]) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = 0usize;
    for tab in tabs {
        if !valid_url(&tab.url) {
            continue;
        }
        let url = tab.url.trim().to_string();
        if url.is_empty() {
            continue;
        }
        if chars + url.len() > 7_500 {
            break;
        }
        chars += url.len() + 1;
        out.push(url);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tab_count_from_browser_titles() {
        assert_eq!(title_tab_count("MES 和另外 4 个页面 - Microsoft Edge"), Some(5));
        assert_eq!(title_tab_count("MES and 3 more pages - Google Chrome"), Some(4));
        assert_eq!(title_tab_count("MES - Google Chrome"), None);
    }

    #[test]
    fn launch_urls_only_http_and_limited() {
        let tabs = vec![
            BrowserTab {
                title: "a".into(),
                url: "https://a.test".into(),
                active: true,
            },
            BrowserTab {
                title: "b".into(),
                url: "chrome://settings".into(),
                active: false,
            },
            BrowserTab {
                title: "c".into(),
                url: "file:///C:/a.html".into(),
                active: false,
            },
        ];
        assert_eq!(
            urls_for_launch(&tabs),
            vec!["https://a.test".to_string(), "file:///C:/a.html".to_string()]
        );
    }
}

#[cfg(windows)]
fn register_host(root: &str, name: &str, manifest: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    let path = format!("{root}\\{name}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let value = manifest
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("RegCreateKeyExW={rc}"));
    }
    let rc = unsafe {
        RegSetValueExW(
            key,
            std::ptr::null(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * std::mem::size_of::<u16>()) as u32,
        )
    };
    unsafe { RegCloseKey(key) };
    if rc != ERROR_SUCCESS {
        return Err(format!("RegSetValueExW={rc}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn register_host(_root: &str, _name: &str, _manifest: &Path) -> Result<(), String> {
    Err("only Windows".into())
}

#[cfg(windows)]
fn registry_installed(root: &str, name: &str) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };
    let path = format!("{root}\\{name}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if rc != ERROR_SUCCESS {
        return false;
    }
    let mut len = 0u32;
    let rc = unsafe {
        windows_sys::Win32::System::Registry::RegQueryValueExW(
            key,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut len,
        )
    };
    unsafe { RegCloseKey(key) };
    rc == ERROR_SUCCESS && len > 0
}

#[cfg(not(windows))]
fn registry_installed(_root: &str, _name: &str) -> bool {
    false
}
