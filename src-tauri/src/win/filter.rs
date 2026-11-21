use super::window::RawWindow;
use crate::core::models::ShowState;

/// 系统 Shell / 系统 UI 窗口类名：这些不是用户的「工作窗口」。
/// 文档 §3.2 要求「过滤系统窗口并标记不确定项」，规则集中在此处便于调整。
const SYSTEM_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "Windows.UI.Composition.DesktopWindowContentBridge",
    "ApplicationFrameWindow",
    "XamlExplorerHostIslandWindow",
    "Windows.Internal.Shell.TabProxyWindow",
    "MultitaskingViewFrame",
    "ForegroundStaging",
    "TaskListThumbnailWnd",
    "InputSiteWindowClass",
    "MSCTFIME UI",
    "IME",
    "NarratorHelperWindow",
    "SysShadow",
    "DV2ControlHost",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "WebViewHost",
];

/// 返回排除原因；None 表示这是一个可捕获的工作窗口。
///
/// `allow_self_window` 仅供自检/模拟应用使用：测试窗口与 WorkScene 同进程，
/// 正常捕获时必须排除自己的窗口，避免把产品界面保存进快照。
pub fn exclude_reason(w: &RawWindow, self_process_id: u32, allow_self_window: bool) -> Option<String> {
    if !w.is_visible {
        return Some("不可见窗口".into());
    }

    // 最小化窗口的 GetWindowRect 会变成 (-32000,-32000) 尺寸约 160x28，
    // 必须使用 WINDOWPLACEMENT.rcNormalPosition 判定真实尺寸，否则所有最小化的浏览器/软件都会被误杀
    let effective = if w.show_state == ShowState::Minimized {
        w.restored_rect
    } else {
        w.rect
    };

    let width = effective.right - effective.left;
    let height = effective.bottom - effective.top;

    if width <= 0 || height <= 0 {
        return Some("零尺寸窗口".into());
    }

    if w.class_name == "ApplicationFrameWindow" {
        return Some("UWP 打包应用，v1.0 不支持窗口恢复".into());
    }

    if SYSTEM_CLASSES.contains(&w.class_name.as_str()) {
        return Some("系统 Shell 窗口".into());
    }

    if w.is_tool_window {
        return Some("工具窗口（WS_EX_TOOLWINDOW）".into());
    }

    if w.pid == self_process_id && !allow_self_window {
        return Some("WorkScene 自身窗口".into());
    }

    if w.title.trim().is_empty() && w.has_owner {
        return Some("无标题的附属弹出窗口".into());
    }

    // 过小的浮层/提示条不是工作窗口
    if width < 120 || height < 90 {
        return Some("尺寸过小的浮层窗口".into());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::ShowState;
    use windows_sys::Win32::Foundation::{HWND, RECT};

    fn hwnd() -> HWND {
        // 过滤逻辑不接触句柄，用非空哨兵值即可
        1isize as HWND
    }

    fn win(class: &str, title: &str, w: i32, h: i32) -> RawWindow {
        let rect = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        RawWindow {
            hwnd: hwnd(),
            pid: 42,
            title: title.into(),
            class_name: class.into(),
            rect,
            restored_rect: rect,
            show_state: ShowState::Normal,
            is_visible: true,
            is_tool_window: false,
            has_owner: false,
        }
    }

    #[test]
    fn keeps_normal_application_window() {
        assert!(
            exclude_reason(
                &win("Chrome_WidgetWin_1", "新标签页 - Google Chrome", 1200, 800),
                7,
                false
            )
            .is_none()
        );
    }

    #[test]
    fn drops_shell_and_uwp() {
        assert!(exclude_reason(&win("Shell_TrayWnd", "任务栏", 1920, 40), 7, false).is_some());
        assert!(
            exclude_reason(&win("ApplicationFrameWindow", "设置", 1000, 700), 7, false).is_some()
        );
    }

    #[test]
    fn drops_self_and_tiny_windows() {
        let mut w = win("Chrome_WidgetWin_1", "x", 800, 600);
        w.pid = 7;
        assert!(exclude_reason(&w, 7, false).is_some());
        assert!(exclude_reason(&w, 7, true).is_none(), "自检模式应允许同进程测试窗口");
        assert!(exclude_reason(&win("SomeClass", "tip", 60, 40), 7, false).is_some());
    }

    #[test]
    fn keeps_minimized_browser_window() {
        let mut w = win("Chrome_WidgetWin_1", "Google Chrome", 160, 28);
        w.rect = RECT {
            left: -32000,
            top: -32000,
            right: -31840,
            bottom: -31972,
        };
        w.restored_rect = RECT {
            left: 100,
            top: 80,
            right: 1500,
            bottom: 980,
        };
        w.show_state = ShowState::Minimized;
        assert!(
            exclude_reason(&w, 7, false).is_none(),
            "最小化浏览器窗口应按 rcNormalPosition 判定并被正常捕获"
        );
    }
}
