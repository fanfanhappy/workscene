//! 原生 Windows 对话框：
//! - `error`: 致命错误提示框（MessageBoxW）
//! - `pick_file`: 原生文件/程序选择器（comdlg32 GetOpenFileNameW）
//! - `pick_folder`: 原生文件夹选择器（shell32 SHBrowseForFolderW）
//! 全部直接调用系统 Win32 DLL，不引入额外外部插件依赖。

use std::ffi::c_void;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub fn error(title: &str, message: &str) {
    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let m: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            m.as_ptr(),
            t.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[repr(C)]
struct OpenFileNameW {
    l_struct_size: u32,
    hwnd_owner: *mut c_void,
    h_instance: *mut c_void,
    lpstr_filter: *const u16,
    lpstr_custom_filter: *mut u16,
    n_max_cust_filter: u32,
    n_filter_index: u32,
    lpstr_file: *mut u16,
    n_max_file: u32,
    lpstr_file_title: *mut u16,
    n_max_file_title: u32,
    lpstr_initial_dir: *const u16,
    lpstr_title: *const u16,
    flags: u32,
    n_file_offset: u16,
    n_file_extension: u16,
    lpstr_def_ext: *const u16,
    l_cust_data: isize,
    lpfn_hook: *const c_void,
    lp_template_name: *const u16,
    pv_reserved: *mut c_void,
    dw_reserved: u32,
    flags_ex: u32,
}

#[repr(C)]
struct BrowseInfoW {
    hwnd_owner: *mut c_void,
    pidl_root: *const c_void,
    psz_display_name: *mut u16,
    lpsz_title: *const u16,
    ul_flags: u32,
    lpfn: *const c_void,
    l_param: isize,
    i_image: i32,
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(lpofn: *mut OpenFileNameW) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn SHBrowseForFolderW(lpbi: *mut BrowseInfoW) -> *mut c_void;
    fn SHGetPathFromIDListW(pidl: *const c_void, psz_path: *mut u16) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(pv: *mut c_void);
}

const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_EXPLORER: u32 = 0x0008_0000;

const BIF_RETURNONLYFSDIRS: u32 = 0x0000_0001;
const BIF_EDITBOX: u32 = 0x0000_0010;
const BIF_NEWDIALOGSTYLE: u32 = 0x0000_0040;
const COINIT_APARTMENTTHREADED: u32 = 0x2;

/// 弹出 Windows 原生文件选择框；`exe_only = true` 时筛选可执行程序
pub fn pick_file(title: &str, exe_only: bool) -> Option<String> {
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let filter_str = if exe_only {
        "应用程序 (*.exe)\0*.exe\0所有文件 (*.*)\0*.*\0\0"
    } else {
        "所有文件 (*.*)\0*.*\0\0"
    };
    let filter_w: Vec<u16> = filter_str.encode_utf16().collect();
    let mut buf: Vec<u16> = vec![0u16; 4096];

    let mut ofn = OpenFileNameW {
        l_struct_size: std::mem::size_of::<OpenFileNameW>() as u32,
        hwnd_owner: std::ptr::null_mut(),
        h_instance: std::ptr::null_mut(),
        lpstr_filter: filter_w.as_ptr(),
        lpstr_custom_filter: std::ptr::null_mut(),
        n_max_cust_filter: 0,
        n_filter_index: 1,
        lpstr_file: buf.as_mut_ptr(),
        n_max_file: buf.len() as u32,
        lpstr_file_title: std::ptr::null_mut(),
        n_max_file_title: 0,
        lpstr_initial_dir: std::ptr::null(),
        lpstr_title: title_w.as_ptr(),
        flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        n_file_offset: 0,
        n_file_extension: 0,
        lpstr_def_ext: std::ptr::null(),
        l_cust_data: 0,
        lpfn_hook: std::ptr::null(),
        lp_template_name: std::ptr::null(),
        pv_reserved: std::ptr::null_mut(),
        dw_reserved: 0,
        flags_ex: 0,
    };

    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..len]);
    if path.trim().is_empty() {
        None
    } else {
        Some(path)
    }
}

/// 弹出 Windows 原生文件夹选择框
pub fn pick_folder(title: &str) -> Option<String> {
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mut display_buf: Vec<u16> = vec![0u16; 512];
    let mut path_buf: Vec<u16> = vec![0u16; 4096];

    unsafe {
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let need_uninit = hr == 0 || hr == 1;

        let mut bi = BrowseInfoW {
            hwnd_owner: std::ptr::null_mut(),
            pidl_root: std::ptr::null(),
            psz_display_name: display_buf.as_mut_ptr(),
            lpsz_title: title_w.as_ptr(),
            ul_flags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
            lpfn: std::ptr::null(),
            l_param: 0,
            i_image: 0,
        };

        let pidl = SHBrowseForFolderW(&mut bi);
        let result = if !pidl.is_null() {
            let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
            CoTaskMemFree(pidl);
            if ok != 0 {
                let len = path_buf
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(path_buf.len());
                let path = String::from_utf16_lossy(&path_buf[..len]);
                if path.trim().is_empty() {
                    None
                } else {
                    Some(path)
                }
            } else {
                None
            }
        } else {
            None
        };

        if need_uninit {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_struct_sizes_match_x64_abi() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(std::mem::size_of::<OpenFileNameW>(), 152);
            assert_eq!(std::mem::size_of::<BrowseInfoW>(), 64);
        }
    }
}
