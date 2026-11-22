//! 应用适配器的 L1/L2 实现（PRD §11）。
//!
//! - L1 通用：任何 Win32 应用都能启动、等窗口、定位（由恢复引擎实现）；
//! - L2 参数：按可执行文件名识别主流 IDE / 终端 / 浏览器 / 资源管理器，
//!   在用户确认后给出规范的启动参数建议（例如 wt.exe 使用 `-d`，Firefox 使用 `-new-window`），
//!   不猜、不覆盖用户已填内容。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    VsCode,
    JetBrains,
    Terminal,
    Browser,
    Explorer,
    Generic,
}

impl AdapterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AdapterKind::VsCode => "vscode",
            AdapterKind::JetBrains => "jetbrains",
            AdapterKind::Terminal => "terminal",
            AdapterKind::Browser => "browser",
            AdapterKind::Explorer => "explorer",
            AdapterKind::Generic => "generic",
        }
    }
}

/// 依据可执行文件名识别适配等级（大小写不敏感）
pub fn classify(exe_name: &str) -> AdapterKind {
    let name = exe_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe_name)
        .trim()
        .to_lowercase();
    match name.as_str() {
        "code.exe"
        | "code - insiders.exe"
        | "cursor.exe"
        | "antigravity.exe"
        | "windsurf.exe"
        | "trae.exe" => AdapterKind::VsCode,
        "idea64.exe"
        | "pycharm64.exe"
        | "webstorm64.exe"
        | "goland64.exe"
        | "clion64.exe"
        | "rider64.exe"
        | "rustrover64.exe"
        | "datagrip64.exe"
        | "phpstorm64.exe" => AdapterKind::JetBrains,
        "windowsterminal.exe" | "wt.exe" | "cmd.exe" | "powershell.exe" | "pwsh.exe" => {
            AdapterKind::Terminal
        }
        "chrome.exe"
        | "msedge.exe"
        | "firefox.exe"
        | "brave.exe"
        | "arc.exe"
        | "vivaldi.exe"
        | "opera.exe"
        | "360chrome.exe"
        | "360chromex.exe" => AdapterKind::Browser,
        "explorer.exe" => AdapterKind::Explorer,
        _ => AdapterKind::Generic,
    }
}

/// 是否属于可直接打开项目目录的 IDE（供恢复文件夹资源时自动选择 IDE）
#[allow(dead_code)]
pub fn is_ide(exe_name: &str) -> bool {
    matches!(classify(exe_name), AdapterKind::VsCode | AdapterKind::JetBrains)
}

/// 给出"打开某资源"时的建议参数（仅建议，需用户确认）
pub fn suggest_args(exe_name: &str, target: Option<&str>) -> Option<String> {
    let target = target?.trim();
    if target.is_empty() {
        return None;
    }
    let name = exe_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe_name)
        .trim()
        .to_lowercase();

    match classify(&name) {
        // VS Code / Cursor / Antigravity / JetBrains：直接传项目目录或文件路径
        AdapterKind::VsCode | AdapterKind::JetBrains => Some(quote_if_needed(target)),
        // Windows Terminal 必须用 -d 指定初始工作目录；pwsh 使用 -WorkingDirectory
        AdapterKind::Terminal => match name.as_str() {
            "windowsterminal.exe" | "wt.exe" => Some(format!("-d {}", quote_if_needed(target))),
            "pwsh.exe" => Some(format!("-WorkingDirectory {}", quote_if_needed(target))),
            _ => None,
        },
        // 浏览器：Firefox 用 -new-window，Chromium 系用 --new-window
        AdapterKind::Browser => {
            if name == "firefox.exe" {
                Some(format!("-new-window {}", quote_if_needed(target)))
            } else {
                Some(format!("--new-window {}", quote_if_needed(target)))
            }
        }
        // 资源管理器：直接把路径作为参数
        AdapterKind::Explorer => Some(quote_if_needed(target)),
        AdapterKind::Generic => None,
    }
}

fn quote_if_needed(s: &str) -> String {
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_apps() {
        assert_eq!(classify("Code.exe"), AdapterKind::VsCode);
        assert_eq!(classify("Antigravity.exe"), AdapterKind::VsCode);
        assert_eq!(classify("Windsurf.exe"), AdapterKind::VsCode);
        assert_eq!(classify("idea64.exe"), AdapterKind::JetBrains);
        assert_eq!(classify("WindowsTerminal.exe"), AdapterKind::Terminal);
        assert_eq!(classify("chrome.exe"), AdapterKind::Browser);
        assert_eq!(classify("Arc.exe"), AdapterKind::Browser);
        assert_eq!(classify("explorer.exe"), AdapterKind::Explorer);
        assert_eq!(classify("notepad.exe"), AdapterKind::Generic);
        assert!(is_ide("Antigravity.exe"));
        assert!(is_ide("pycharm64.exe"));
        assert!(!is_ide("chrome.exe"));
    }

    #[test]
    fn suggests_args_only_when_target_given() {
        assert!(suggest_args("Code.exe", None).is_none());
        assert!(suggest_args("Code.exe", Some("  ")).is_none());
        assert_eq!(
            suggest_args("Code.exe", Some(r"D:\work\app")).as_deref(),
            Some(r"D:\work\app")
        );
        assert_eq!(
            suggest_args("WindowsTerminal.exe", Some(r"D:\work\my app")).as_deref(),
            Some(r#"-d "D:\work\my app""#)
        );
        assert_eq!(
            suggest_args("firefox.exe", Some("https://example.com")).as_deref(),
            Some("-new-window https://example.com")
        );
    }

    #[test]
    fn quotes_paths_with_spaces_and_browsers_get_new_window() {
        assert_eq!(
            suggest_args("Code.exe", Some(r"D:\my work\app")).as_deref(),
            Some("\"D:\\my work\\app\"")
        );
        assert_eq!(
            suggest_args("chrome.exe", Some("https://example.com")).as_deref(),
            Some("--new-window https://example.com")
        );
    }

    #[test]
    fn generic_apps_get_no_suggestion() {
        assert!(suggest_args("notepad.exe", Some(r"C:\a.txt")).is_none());
    }
}
