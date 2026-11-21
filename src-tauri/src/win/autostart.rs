//! 开机启动：写 HKCU\Software\Microsoft\Windows\CurrentVersion\Run。
//!
//! 为什么用注册表而不是计划任务/启动文件夹：
//! - 只对当前用户生效（不需要管理员）；
//! - 值名固定，卸载/关闭时可精确删除，不会留垃圾；
//! - 命令行带引号，避免路径含空格时启动失败。

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WorkScene";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_run_key_verbose(access: u32) -> Result<HKEY, u32> {
    let path = wide(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
    if rc == ERROR_SUCCESS {
        Ok(key)
    } else {
        Err(rc)
    }
}

fn open_run_key(access: u32) -> Option<HKEY> {
    open_run_key_verbose(access).ok()
}

/// 本程序启动命令（带引号，防止路径含空格）
pub fn launch_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.to_string_lossy()))
}

pub fn is_enabled() -> bool {
    let Some(key) = open_run_key(KEY_QUERY_VALUE) else {
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

    if rc == ERROR_FILE_NOT_FOUND {
        return false;
    }
    if rc != ERROR_SUCCESS {
        return false;
    }

    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let current = String::from_utf16_lossy(&buf[..end]);
    // 值存在但指向别的路径（例如旧版本安装位置）也算需要修
    launch_command()
        .map(|expected| current.eq_ignore_ascii_case(&expected))
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> bool {
    set_enabled_verbose(enabled).is_ok()
}

/// 返回 Windows 错误码，便于诊断（0 表示成功）
pub fn set_enabled_verbose(enabled: bool) -> Result<(), u32> {
    let key = open_run_key_verbose(KEY_SET_VALUE)?;
    let name = wide(VALUE_NAME);

    let rc = if enabled {
        let Some(command) = launch_command() else {
            unsafe { RegCloseKey(key) };
            return Err(2);
        };
        let value = wide(&command);
        unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * size_of::<u16>()) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, name.as_ptr()) }
    };

    unsafe { RegCloseKey(key) };

    if rc == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(rc)
    }
}

/// 让注册表与期望状态一致（设置页开关、启动时自检都用它）
pub fn sync(enabled: bool) -> bool {
    if enabled == is_enabled() {
        return true;
    }
    set_enabled(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_command_is_quoted() {
        // 只验证格式：必须被引号包住，避免空格路径被拆成多个参数
        if let Some(cmd) = launch_command() {
            assert!(cmd.starts_with('"') && cmd.ends_with('"'), "命令必须是带引号的完整路径");
        }
    }
}
