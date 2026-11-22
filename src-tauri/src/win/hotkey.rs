//! 全局快捷键：预设组合 + 运行时重新注册。
//!
//! RegisterHotKey 是**线程绑定**的：WM_HOTKEY 只会投递到注册它的那个线程。
//! 所以改快捷键必须由持有消息循环的那个线程去 Unregister/Register，
//! 设置页只能通过通道把"换成哪个组合"发过去。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, sync_channel, Sender, SyncSender};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY,
};

const WM_HOTKEY_RELOAD: u32 = WM_APP + 0x42;

pub const HOTKEY_ID: i32 = 0x5753; // 'WS'
pub const HOTKEY_WS_BASE: i32 = 0x5760; // Ctrl+Alt+1 .. Ctrl+Alt+9
pub const WS_HOTKEY_COUNT: usize = 9;

/// 支持的预设组合（白名单，避免任意字符串注册失败）
pub const PRESETS: [&str; 4] = ["Ctrl+Alt+W", "Ctrl+Shift+W", "Ctrl+Alt+Space", ""];

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_NOREPEAT: u32 = 0x4000;

pub const VK_SPACE: u32 = 0x20;
pub const VK_1: u32 = 0x31;
pub const VK_W: u32 = 0x57;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub main_combo: Option<Combo>,
    pub workspace_slots: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: u32,
    pub vk: u32,
}

/// 预设字符串 → 组合键；空串表示关闭
pub fn parse_preset(text: &str) -> Option<Combo> {
    match text.trim() {
        "Ctrl+Alt+W" => Some(Combo {
            modifiers: MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            vk: VK_W,
        }),
        "Ctrl+Shift+W" => Some(Combo {
            modifiers: MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            vk: VK_W,
        }),
        "Ctrl+Alt+Space" => Some(Combo {
            modifiers: MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            vk: VK_SPACE,
        }),
        _ => None,
    }
}

pub fn normalize_preset(text: &str) -> String {
    let t = text.trim();
    if PRESETS.contains(&t) {
        t.to_string()
    } else {
        "Ctrl+Alt+W".to_string()
    }
}

struct HotkeyRequest {
    config: HotkeyConfig,
    reply: SyncSender<bool>,
}

static SENDER: OnceLock<Mutex<Sender<HotkeyRequest>>> = OnceLock::new();
static THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// 窗口线程调用一次：注册初始组合（主唤起键 + 可选 Ctrl+Alt+1~9 空间切屏键）并进入零轮询阻塞消息循环
pub fn spawn(app: tauri::AppHandle, initial: Option<Combo>, workspace_slots: bool) {
    let (tx, rx) = channel::<HotkeyRequest>();
    let _ = SENDER.set(Mutex::new(tx));

    std::thread::spawn(move || unsafe {
        // 强制初始化当前线程消息队列，再公开 thread id 供 PostThreadMessageW 立即唤醒
        let mut init_msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut init_msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);

        let mut current_main = register(initial);
        let mut current_slots = if workspace_slots {
            register_ws_slots()
        } else {
            false
        };

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY_RELOAD {
                while let Ok(req) = rx.try_recv() {
                    let next = req.config;
                    if current_main {
                        unregister();
                        current_main = false;
                    }
                    let main_ok = match next.main_combo {
                        Some(c) => {
                            current_main = register(Some(c));
                            current_main
                        }
                        None => true,
                    };

                    let mut slots_ok = true;
                    if current_slots != next.workspace_slots {
                        if current_slots {
                            unregister_ws_slots();
                            current_slots = false;
                        } else {
                            current_slots = register_ws_slots();
                            slots_ok = current_slots;
                        }
                    }

                    let _ = req.reply.send(main_ok && slots_ok);
                }
            } else if msg.message == WM_HOTKEY {
                let id = msg.wParam as i32;
                if id == HOTKEY_ID {
                    crate::win::tray::show_main(&app);
                } else if (HOTKEY_WS_BASE..HOTKEY_WS_BASE + WS_HOTKEY_COUNT as i32).contains(&id)
                {
                    let slot = (id - HOTKEY_WS_BASE) as i64 + 1;
                    let handle = app.clone();
                    std::thread::spawn(move || {
                        crate::win::tray::restore_workspace_by_slot(&handle, slot);
                    });
                }
            }
        }
    });
}

/// 运行中切换快捷键配置（设置页调用）：通过 PostThreadMessageW 立即唤醒阻塞线程并返回真实 RegisterHotKey 结果
pub fn update(combo: Option<Combo>, workspace_slots: bool) -> bool {
    let Some(tx) = SENDER.get() else {
        return false;
    };
    let (reply_tx, reply_rx) = sync_channel(1);
    let sent = tx
        .lock()
        .map(|s| {
            s.send(HotkeyRequest {
                config: HotkeyConfig {
                    main_combo: combo,
                    workspace_slots,
                },
                reply: reply_tx,
            })
            .is_ok()
        })
        .unwrap_or(false);
    if !sent {
        return false;
    }
    let tid = THREAD_ID.load(Ordering::Acquire);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_HOTKEY_RELOAD, 0, 0);
        }
    }
    reply_rx
        .recv_timeout(std::time::Duration::from_millis(800))
        .unwrap_or(false)
}

unsafe fn register(combo: Option<Combo>) -> bool {
    let Some(c) = combo else {
        return false;
    };
    RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, c.modifiers, c.vk) != 0
}

unsafe fn unregister() {
    UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
}

unsafe fn register_ws_slots() -> bool {
    let mut any = false;
    for i in 0..WS_HOTKEY_COUNT {
        let ok = RegisterHotKey(
            std::ptr::null_mut(),
            HOTKEY_WS_BASE + i as i32,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            VK_1 + i as u32,
        ) != 0;
        if ok {
            any = true;
        }
    }
    any
}

unsafe fn unregister_ws_slots() {
    for i in 0..WS_HOTKEY_COUNT {
        UnregisterHotKey(std::ptr::null_mut(), HOTKEY_WS_BASE + i as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_presets_only() {
        assert_eq!(parse_preset("Ctrl+Alt+W").map(|c| c.vk), Some(VK_W));
        assert_eq!(parse_preset("Ctrl+Shift+W").map(|c| c.vk), Some(VK_W));
        assert_eq!(parse_preset("Ctrl+Alt+Space").map(|c| c.vk), Some(VK_SPACE));
        // 关闭 = 不注册
        assert!(parse_preset("").is_none());
        // 未知值不应被当成合法组合
        assert!(parse_preset("Ctrl+Q").is_none());
        assert!(parse_preset("alt+f4").is_none());
    }

    #[test]
    fn normalize_falls_back_to_default() {
        assert_eq!(normalize_preset("Ctrl+Shift+W"), "Ctrl+Shift+W");
        assert_eq!(normalize_preset(""), "");
        assert_eq!(normalize_preset("随便写的"), "Ctrl+Alt+W");
    }

    #[test]
    fn modifiers_include_no_repeat() {
        let c = parse_preset("Ctrl+Alt+W").unwrap();
        assert_eq!(c.modifiers & MOD_NOREPEAT, MOD_NOREPEAT);
        assert_eq!(c.modifiers & MOD_CONTROL, MOD_CONTROL);
        assert_eq!(c.modifiers & MOD_ALT, MOD_ALT);
    }
}
