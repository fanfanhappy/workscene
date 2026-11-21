//! 应用适配器的最小实现（PRD §11 的 L1/L2）。
//!
//! 这里刻意只做**建议**，不自动接管启动：
//! - L1 通用：任何应用都能启动、等窗口、定位（已由恢复引擎实现）；
//! - L2 参数：按可执行文件名给出"该应用打开某资源时的常见参数"，
//!   由用户确认后写进 app_configs.launch_args，不猜、不覆盖用户已填的内容。
//!
//! 为什么不自动填：不同版本/安装方式的参数不保证一致（例如 Chrome 的
//! --new-window 在已有实例时的行为、VS Code 的 CLI 是否在 PATH 中），
//! 猜错会让"恢复"变成"乱开窗口"，违背不谎报原则。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    VsCode,
    Terminal,
    Browser,
    Explorer,
    Generic,
}

impl AdapterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AdapterKind::VsCode => "vscode",
            AdapterKind::Terminal => "terminal",
            AdapterKind::Browser => "browser",
            AdapterKind::Explorer => "explorer",
            AdapterKind::Generic => "generic",
        }
    }
}

/// 依据可执行文件名识别适配等级（大小写不敏感）
pub fn classify(exe_name: &str) -> AdapterKind {
    let name = exe_name.to_lowercase();
    match name.as_str() {
        "code.exe" | "code - insiders.exe" | "cursor.exe" => AdapterKind::VsCode,
        "windowsterminal.exe" | "wt.exe" | "cmd.exe" | "powershell.exe" | "pwsh.exe" => {
            AdapterKind::Terminal
        }
        "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" => AdapterKind::Browser,
        "explorer.exe" => AdapterKind::Explorer,
        _ => AdapterKind::Generic,
    }
}

/// 给出"打开某资源"时的建议参数（仅建议，需用户确认）
pub fn suggest_args(exe_name: &str, target: Option<&str>) -> Option<String> {
    let target = target?.trim();
    if target.is_empty() {
        return None;
    }
    match classify(exe_name) {
        // VS Code / Terminal：第一个位置参数就是目录或文件
        AdapterKind::VsCode | AdapterKind::Terminal => Some(quote_if_needed(target)),
        // 浏览器：明确要求新窗口打开网址
        AdapterKind::Browser => Some(format!("--new-window {}", quote_if_needed(target))),
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
        assert_eq!(classify("WindowsTerminal.exe"), AdapterKind::Terminal);
        assert_eq!(classify("chrome.exe"), AdapterKind::Browser);
        assert_eq!(classify("explorer.exe"), AdapterKind::Explorer);
        assert_eq!(classify("notepad.exe"), AdapterKind::Generic);
    }

    #[test]
    fn suggests_args_only_when_target_given() {
        assert!(suggest_args("Code.exe", None).is_none());
        assert!(suggest_args("Code.exe", Some("  ")).is_none());
        assert_eq!(
            suggest_args("Code.exe", Some(r"D:\work\app")).as_deref(),
            Some(r"D:\work\app")
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
