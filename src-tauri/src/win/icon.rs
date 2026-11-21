//! Windows 程序真实图标提取（零额外依赖）：
//! - 通过 Win32 `ExtractIconExW` + `CreateDIBSection` + `DrawIconEx` 提取 32×32 RGBA 图标；
//! - 使用轻量纯 Rust 无压缩 PNG + Base64 编码为 `data:image/png;base64,...`；
//! - 全局内存缓存，同一 `.exe` 路径只提取一次。

use std::collections::HashMap;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL, HICON};

const ICON_SIZE: usize = 32;

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 批量提取可执行文件图标，返回 `path -> data:image/png;base64,...`。
pub fn extract_batch(paths: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if let Ok(guard) = cache().lock() {
            if let Some(hit) = guard.get(&key) {
                if let Some(url) = hit {
                    out.insert(raw.clone(), url.clone());
                }
                continue;
            }
        }

        let extracted = extract_one(trimmed);
        if let Ok(mut guard) = cache().lock() {
            if guard.len() > 256 {
                guard.clear();
            }
            guard.insert(key, extracted.clone());
        }
        if let Some(url) = extracted {
            out.insert(raw.clone(), url);
        }
    }
    out
}

fn extract_one(path: &str) -> Option<String> {
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    let wide = to_wide(path);
    unsafe {
        let mut large: HICON = std::ptr::null_mut();
        let mut small: HICON = std::ptr::null_mut();
        let count = ExtractIconExW(wide.as_ptr(), 0, &mut large, &mut small, 1);
        if count == 0 {
            return None;
        }
        let hicon = if !large.is_null() { large } else { small };
        let rgba = if !hicon.is_null() {
            render_hicon_rgba(hicon)
        } else {
            None
        };
        if !large.is_null() {
            DestroyIcon(large);
        }
        if !small.is_null() {
            DestroyIcon(small);
        }
        let rgba = rgba?;
        let png = encode_png_rgba(ICON_SIZE as u32, ICON_SIZE as u32, &rgba);
        Some(format!("data:image/png;base64,{}", base64_encode(&png)))
    }
}

unsafe fn render_hicon_rgba(hicon: HICON) -> Option<Vec<u8>> {
    let hdc = CreateCompatibleDC(std::ptr::null_mut());
    if hdc.is_null() {
        return None;
    }

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: ICON_SIZE as i32,
        // 负高度 = top-down DIB，像素直接从上到下排列
        biHeight: -(ICON_SIZE as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hbm = CreateDIBSection(
        hdc,
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        std::ptr::null_mut(),
        0,
    );
    if hbm.is_null() || bits.is_null() {
        DeleteDC(hdc);
        return None;
    }

    let old = SelectObject(hdc, hbm);
    let byte_len = ICON_SIZE * ICON_SIZE * 4;
    std::ptr::write_bytes(bits as *mut u8, 0, byte_len);

    let ok = DrawIconEx(
        hdc,
        0,
        0,
        hicon,
        ICON_SIZE as i32,
        ICON_SIZE as i32,
        0,
        std::ptr::null_mut(),
        DI_NORMAL,
    );

    let mut rgba = vec![0u8; byte_len];
    let mut has_alpha = false;
    let mut has_color = false;
    if ok != 0 {
        let src = std::slice::from_raw_parts(bits as *const u8, byte_len);
        for (dst_px, src_px) in rgba.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            // Win32 DIB 为 BGRA，转为 PNG RGBA
            dst_px[0] = src_px[2];
            dst_px[1] = src_px[1];
            dst_px[2] = src_px[0];
            dst_px[3] = src_px[3];
            if src_px[3] != 0 {
                has_alpha = true;
            }
            if src_px[0] != 0 || src_px[1] != 0 || src_px[2] != 0 {
                has_color = true;
            }
        }
        // 老程序图标可能没写 alpha 通道（全 0），非黑像素补为不透明
        if !has_alpha && has_color {
            for px in rgba.chunks_exact_mut(4) {
                if px[0] != 0 || px[1] != 0 || px[2] != 0 {
                    px[3] = 255;
                }
            }
        }
    }

    SelectObject(hdc, old);
    DeleteObject(hbm);
    DeleteDC(hdc);

    if ok == 0 || (!has_alpha && !has_color) {
        None
    } else {
        Some(rgba)
    }
}

/// 零依赖标准 RGBA PNG 编码（使用 zlib stored block，32×32 约 4.2KB）
fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row_bytes = (width as usize) * 4;
    let mut raw = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw.push(0u8); // filter type 0 (None)
        raw.extend_from_slice(&rgba[y * row_bytes..(y + 1) * row_bytes]);
    }

    // zlib header (CMF=0x78, FLG=0x01) + stored deflate blocks + Adler32
    let mut zlib = Vec::with_capacity(raw.len() + 32);
    zlib.push(0x78);
    zlib.push(0x01);
    let mut offset = 0usize;
    while offset < raw.len() {
        let chunk_len = (raw.len() - offset).min(65535);
        let is_final = offset + chunk_len >= raw.len();
        zlib.push(if is_final { 0x01 } else { 0x00 });
        let len_u16 = chunk_len as u16;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&(!len_u16).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + chunk_len]);
        offset += chunk_len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32(tag, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(tag: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in tag.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_valid_png_signature_and_base64() {
        let px = vec![255u8, 0, 128, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255];
        let png = encode_png_rgba(2, 2, &px);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert_eq!(&png[12..16], b"IHDR");
        let b64 = base64_encode(&png);
        assert!(b64.starts_with("iVBORw0KGgo"));
    }

    #[test]
    fn base64_matches_standard_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
