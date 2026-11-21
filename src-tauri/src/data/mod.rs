pub mod runs;
pub mod workspaces;

use crate::core::{AppError, AppResult};
use crate::storage;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

/// 数据库句柄，作为 Tauri 的全局状态注入命令。
#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

pub async fn init() -> AppResult<Db> {
    let dir = storage::data_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("workscene.db");

    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|e| AppError::msg(format!("数据库路径无效：{e}")))?
        .create_if_missing(true)
        // WAL：允许读并发；写入仍由单连接事务串行化（PRD §8.5）
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(5000))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
        .map_err(|e| AppError::msg(format!("无法打开数据库：{e}")))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::msg(format!("数据库迁移失败：{e}")))?;

    Ok(Db { pool })
}

/// 测试用内存库：同样跑真实迁移，避免测试与生产 schema 分叉。
#[cfg(test)]
pub async fn in_memory() -> AppResult<Db> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(|e| AppError::msg(e.to_string()))?
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| AppError::msg(format!("无法创建内存数据库：{e}")))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::msg(format!("迁移失败：{e}")))?;

    Ok(Db { pool })
}
