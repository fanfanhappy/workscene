//! 本地诊断日志：只写本机，不上传。
//!
//! 日志按天滚动，默认保留 7 天；设置页可打开日志目录并导出诊断包。
//! 默认级别对 WorkScene 自身是 debug，足够看到“为什么这个窗口被跳过”，
//! 但不记录屏幕像素或浏览器内容。

use crate::core::time::now_ms;
use crate::core::AppResult;
use crate::storage;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;

static INIT: Once = Once::new();

pub fn log_dir() -> PathBuf {
    storage::data_dir().join("logs")
}

fn cleanup_old_logs(dir: &Path) {
    const KEEP_DAYS: u64 = 7;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("workscene.log"))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let age = now.duration_since(modified).unwrap_or_default();
        if age.as_secs() > KEEP_DAYS * 86_400 {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// 初始化全局日志；重复调用安全。必须在创建窗口前调用。
pub fn init() {
    INIT.call_once(|| {
        let dir = log_dir();
        let _ = std::fs::create_dir_all(&dir);
        cleanup_old_logs(&dir);

        let filter = std::env::var("WORKSCENE_LOG")
            .ok()
            .and_then(|v| tracing_subscriber::EnvFilter::try_new(v).ok())
            .or_else(|| tracing_subscriber::EnvFilter::try_from_default_env().ok())
            .unwrap_or_else(|| {
                tracing_subscriber::EnvFilter::new("info,workscene_lib=debug")
            });

        let appender = tracing_appender::rolling::daily(&dir, "workscene.log");
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(appender)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .try_init();

        // panic 是 release 版最难排查的一类问题：把栈信息写进同一个日志文件。
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!(panic = %info, "unhandled panic");
            default_hook(info);
        }));

        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            data_dir = %storage::data_dir().display(),
            log_dir = %dir.display(),
            "WorkScene logging initialized"
        );
    });
}

/// 诊断包临时目录（每次导出独立，避免覆盖旧包）。
pub fn new_diagnostics_dir() -> AppResult<PathBuf> {
    let dir = storage::data_dir()
        .join("diagnostics")
        .join(format!("diag-{:x}", now_ms()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn diagnostics_zip_path() -> PathBuf {
    storage::data_dir()
        .join("diagnostics")
        .join(format!("WorkScene-diagnostics-{:x}.zip", now_ms()))
}

/// 把日志文件复制进诊断目录；即使日志还没写入也不报错。
pub fn copy_logs(dest: &Path) -> AppResult<usize> {
    let dir = log_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(0);
    };
    let mut copied = 0usize;
    std::fs::create_dir_all(dest)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("workscene.log"))
            .unwrap_or(false)
        {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        if std::fs::copy(&path, dest.join(name)).is_ok() {
            copied += 1;
        }
    }
    Ok(copied)
}

/// 递归压缩目录，条目名使用相对路径，避免把用户目录结构带进 zip。
pub fn zip_dir(src: &Path, dest: &Path) -> AppResult<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    add_dir_to_zip(&mut zip, src, src, options)?;
    zip.finish()
        .map_err(|e| crate::core::AppError::msg(format!("关闭诊断压缩包失败：{e}")))?;
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
) -> AppResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(zip, root, &path, options)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| crate::core::AppError::msg("诊断文件路径越界"))?;
        let name = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(name, options)
            .map_err(|e| crate::core::AppError::msg(format!("写入诊断压缩包失败：{e}")))?;
        let mut f = std::fs::File::open(&path)?;
        std::io::copy(&mut f, zip)
            .map_err(|e| crate::core::AppError::msg(format!("读取诊断文件失败：{e}")))?;
    }
    Ok(())
}

/// 写一个 JSON 文件到诊断目录，格式错误直接暴露给调用方。
pub fn write_json<T: serde::Serialize>(dir: &Path, name: &str, value: &T) -> AppResult<()> {
    let path = dir.join(name);
    let json = serde_json::to_string_pretty(value)?;
    let mut f = std::fs::File::create(path)?;
    f.write_all(json.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_dir_contains_nested_files() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".testdata")
            .join(format!("workscene-zip-test-{:x}", now_ms()));
        let src = base.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.json"), b"{}").unwrap();
        std::fs::write(src.join("sub").join("b.log"), b"hello").unwrap();
        let zip_path = base.join("diag.zip");
        zip_dir(&src, &zip_path).unwrap();
        let meta = std::fs::metadata(&zip_path).unwrap();
        assert!(meta.len() > 0);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(archive.by_name("a.json").is_ok());
        assert!(archive.by_name("sub/b.log").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }
}
