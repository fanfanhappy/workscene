//! 开机启动：优先写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`；
//! 若本机安全软件或组策略拦截注册表 `Run` 写入（常见 `ERROR_ACCESS_DENIED = 5`），
//! 自动回退到当前用户的系统「启动」文件夹（`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WorkScene.vbs`）。
//!
//! 特点：
//! - 仅对当前用户生效，无需管理员权限；
//! - `.vbs` 启动脚本使用 `WScript.Shell.Run ..., 0, False`，启动时无黑框闪烁；
//! - 关闭开关时同时清理注册表与启动文件夹，不留残留。

use std::path::PathBuf;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WorkScene";
const STARTUP_SCRIPT_NAME: &str = "WorkScene.vbs";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_run_key_read() -> Option<HKEY> {
    let path = wide(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_QUERY_VALUE, &mut key) };
    if rc == ERROR_SUCCESS {
        Some(key)
    } else {
        None
    }
}

fn create_or_open_run_key_write() -> Result<HKEY, u32> {
    let path = wide(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(key)
    } else {
        Err(rc)
    }
}

/// 本程序启动命令（带引号，防止路径含空格）
pub fn launch_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.to_string_lossy()))
}

fn startup_script_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
            .join(STARTUP_SCRIPT_NAME),
    )
}

fn build_vbs_script(exe_path: &str, data_dir: Option<&str>) -> String {
    let escaped_exe = exe_path.replace('"', "\"\"");
    let mut script = String::from("Set sh = CreateObject(\"WScript.Shell\")\r\n");
    if let Some(dir) = data_dir {
        if !dir.trim().is_empty() {
            let escaped_dir = dir.replace('"', "\"\"");
            script.push_str(&format!(
                "sh.Environment(\"Process\")(\"WORKSCENE_DATA_DIR\") = \"{escaped_dir}\"\r\n"
            ));
        }
    }
    script.push_str(&format!("sh.Run \"\"\"{escaped_exe}\"\"\", 0, False\r\n"));
    script
}

fn is_registry_enabled() -> bool {
    let Some(key) = open_run_key_read() else {
        return false;
    };
    let name = wide(VALUE_NAME);
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * size_of::<u16>()) as u32;
    let rc = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut u8,
            &mut len,
        )
    };
    unsafe { RegCloseKey(key) };

    if rc != ERROR_SUCCESS {
        return false;
    }

    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let current = String::from_utf16_lossy(&buf[..end]);
    launch_command()
        .map(|expected| current.eq_ignore_ascii_case(&expected))
        .unwrap_or(false)
}

fn is_startup_folder_enabled() -> bool {
    let Some(path) = startup_script_path() else {
        return false;
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    content
        .to_lowercase()
        .contains(&exe.to_string_lossy().to_lowercase())
}

pub fn is_enabled() -> bool {
    is_registry_enabled() || is_startup_folder_enabled()
}

fn write_registry(command: &str) -> Result<(), u32> {
    let key = create_or_open_run_key_write()?;
    let name = wide(VALUE_NAME);
    let value = wide(command);
    let rc = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * size_of::<u16>()) as u32,
        )
    };
    unsafe { RegCloseKey(key) };
    if rc == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(rc)
    }
}

fn delete_registry() -> Result<(), u32> {
    let key = match create_or_open_run_key_write() {
        Ok(k) => k,
        Err(ERROR_FILE_NOT_FOUND) => return Ok(()),
        Err(e) => return Err(e),
    };
    let name = wide(VALUE_NAME);
    let rc = unsafe { RegDeleteValueW(key, name.as_ptr()) };
    unsafe { RegCloseKey(key) };
    if rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(rc)
    }
}

fn write_startup_folder() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法获取程序路径：{e}"))?;
    let path = startup_script_path().ok_or_else(|| "找不到系统启动文件夹（APPDATA 未设置）".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建启动目录：{e}"))?;
    }
    let data_dir = std::env::var("WORKSCENE_DATA_DIR").ok();
    let script = build_vbs_script(&exe.to_string_lossy(), data_dir.as_deref());
    std::fs::write(&path, script).map_err(|e| format!("写入启动文件夹失败：{e}"))
}

fn remove_startup_folder() -> Result<(), String> {
    let Some(path) = startup_script_path() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path).map_err(|e| format!("移除启动脚本失败：{e}"))
}

/// 返回 Windows 错误码（兼容 `--autostart` CLI 诊断，0 表示成功）
pub fn set_enabled_verbose(enabled: bool) -> Result<(), u32> {
    sync_detailed(enabled).map(|_| ()).map_err(|(code, _)| code)
}

/// 详细同步开机启动状态：优先注册表，被安全软件/策略拦截时自动改用启动文件夹
pub fn sync_detailed(enabled: bool) -> Result<(), (u32, String)> {
    if enabled {
        let Some(command) = launch_command() else {
            return Err((2, "无法获取当前可执行文件路径".into()));
        };

        // 如果设置了便携数据目录（WORKSCENE_DATA_DIR），优先用启动文件夹才能连带环境变量一起保留；
        // 否则先写注册表 HKCU\Run，被拦截时自动回退到启动文件夹。
        let has_portable_env = std::env::var_os("WORKSCENE_DATA_DIR")
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        if has_portable_env {
            match write_startup_folder() {
                Ok(()) => {
                    let _ = delete_registry();
                    return Ok(());
                }
                Err(folder_err) => match write_registry(&command) {
                    Ok(()) => return Ok(()),
                    Err(reg_rc) => {
                        return Err((
                            reg_rc,
                            format!(
                                "系统拒绝了开机启动项写入（注册表错误码 {reg_rc}；{folder_err}）。常见原因：安全软件拦截、受限账户/组策略、或程序由权限受限的父进程启动。可点右侧「打开启动文件夹」手动放入快捷方式，或改用管理员身份运行 WorkScene 再开启。"
                            ),
                        ));
                    }
                },
            }
        }

        match write_registry(&command) {
            Ok(()) => {
                let _ = remove_startup_folder();
                Ok(())
            }
            Err(reg_rc) => match write_startup_folder() {
                Ok(()) => Ok(()),
                Err(folder_err) => Err((
                    reg_rc,
                    format!(
                        "系统拒绝了开机启动项写入（注册表错误码 {reg_rc}；{folder_err}）。常见原因：安全软件拦截、受限账户/组策略、或程序由权限受限的父进程启动。可点右侧「打开启动文件夹」手动放入快捷方式，或改用管理员身份运行 WorkScene 再开启。"
                    ),
                )),
            },
        }
    } else {
        let reg_res = delete_registry();
        let folder_res = remove_startup_folder();
        match (reg_res, folder_res) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(rc), Ok(())) => {
                // 如果注册表原本就没有 WorkScene 值，忽略注册表权限错误
                if !is_registry_enabled() {
                    Ok(())
                } else {
                    Err((rc, format!("无法删除注册表启动项（错误码 {rc}）")))
                }
            }
            (_, Err(e)) => Err((5, e)),
        }
    }
}

/// 在资源管理器中打开当前用户的 Windows「启动」文件夹，方便在安全软件拦截时手动拖入快捷方式
pub fn open_startup_folder() -> Result<(), String> {
    let path = startup_script_path()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .ok_or_else(|| "找不到系统启动文件夹（APPDATA 未设置）".to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("无法打开启动文件夹：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断用：手动执行 `cargo test --lib probe_autostart -- --ignored --nocapture`
    /// 会逐层打印写注册表 / 写启动文件夹的真实结果（带路径），用于排查"开关一直失败"。
    #[test]
    #[ignore]
    fn probe_autostart() {
        unsafe {
            match create_or_open_run_key_write() {
                Ok(k) => {
                    println!("[probe] 打开 HKCU\\Run（写权限）成功");
                    RegCloseKey(k);
                }
                Err(rc) => println!("[probe] 打开 HKCU\\Run 失败，Win32 = {rc}"),
            }
        }
        println!("[probe] write_registry => {:?}", write_registry("\"C:\\probe.exe\""));
        println!("[probe] startup_script_path => {:?}", startup_script_path());
        println!("[probe] write_startup_folder => {:?}", write_startup_folder());
        println!("[probe] is_enabled => {}", is_enabled());
        let _ = delete_registry();
        let _ = remove_startup_folder();
    }
    #[test]
    fn launch_command_is_quoted() {
        if let Some(cmd) = launch_command() {
            assert!(cmd.starts_with('"') && cmd.ends_with('"'), "命令必须是带引号的完整路径");
        }
    }

    #[test]
    fn vbs_script_quotes_exe_and_optional_data_dir() {
        let s1 = build_vbs_script(r"D:\My Apps\WorkScene.exe", None);
        assert!(s1.contains(r#"sh.Run """D:\My Apps\WorkScene.exe""", 0, False"#));
        assert!(!s1.contains("WORKSCENE_DATA_DIR"));

        let s2 = build_vbs_script(r"D:\WorkScene\WorkScene.exe", Some(r"D:\WorkScene\data"));
        assert!(s2.contains(r#"sh.Environment("Process")("WORKSCENE_DATA_DIR") = "D:\WorkScene\data""#));
    }
}
