//! 应用设置：全部落在 `app_settings` 的 key/value 上，读取时带默认值。
//! 这里只做"有类型的一层薄封装"，不做业务判断。

use crate::core::time::now_ms;
use crate::core::AppResult;
use crate::data::Db;
use crate::storage;
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const KEY_AUTO_ENABLED: &str = "auto_snapshot_enabled";
pub const KEY_AUTO_KEEP: &str = "auto_keep";
pub const KEY_AUTO_INTERVAL: &str = "auto_min_interval_secs";
pub const KEY_RELAUNCH_DEFAULT: &str = "restore_relaunch_default";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub auto_snapshot_enabled: bool,
    pub auto_keep: i64,
    pub auto_min_interval_secs: i64,
    pub restore_relaunch_default: bool,
    /// 开机启动：真源是注册表（HKCU Run），这里只做展示与提交
    #[serde(default)]
    pub autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            auto_snapshot_enabled: true,
            // 与 ADR-0004 的常量保持一致，改这里要同步文档
            auto_keep: super::snapshots::AUTO_KEEP_PER_WORKSPACE as i64,
            auto_min_interval_secs: super::snapshots::AUTO_MIN_INTERVAL_MS / 1000,
            restore_relaunch_default: true,
            autostart: false,
        }
    }
}

impl AppSettings {
    /// 约束到合理区间，避免用户填出 0 保留数或 1 秒节流
    pub fn sanitized(mut self) -> Self {
        self.auto_keep = self.auto_keep.clamp(1, 500);
        self.auto_min_interval_secs = self.auto_min_interval_secs.clamp(10, 86_400);
        self
    }
}

async fn read_raw(db: &Db, key: &str) -> AppResult<Option<String>> {
    let row = sqlx::query("SELECT value_json FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("value_json")))
}

async fn write_raw(db: &Db, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                        updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now_ms())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get(db: &Db) -> AppResult<AppSettings> {
    let mut s = AppSettings::default();

    if let Some(v) = read_raw(db, KEY_AUTO_ENABLED).await? {
        s.auto_snapshot_enabled = v == "true" || v == "1";
    }
    if let Some(v) = read_raw(db, KEY_AUTO_KEEP).await? {
        if let Ok(n) = v.parse::<i64>() {
            s.auto_keep = n;
        }
    }
    if let Some(v) = read_raw(db, KEY_AUTO_INTERVAL).await? {
        if let Ok(n) = v.parse::<i64>() {
            s.auto_min_interval_secs = n;
        }
    }
    if let Some(v) = read_raw(db, KEY_RELAUNCH_DEFAULT).await? {
        s.restore_relaunch_default = v == "true" || v == "1";
    }

    Ok(s.sanitized())
}

pub async fn set(db: &Db, settings: &AppSettings) -> AppResult<AppSettings> {
    let s = settings.clone().sanitized();

    write_raw(db, KEY_AUTO_ENABLED, if s.auto_snapshot_enabled { "true" } else { "false" }).await?;
    write_raw(db, KEY_AUTO_KEEP, &s.auto_keep.to_string()).await?;
    write_raw(db, KEY_AUTO_INTERVAL, &s.auto_min_interval_secs.to_string()).await?;
    write_raw(
        db,
        KEY_RELAUNCH_DEFAULT,
        if s.restore_relaunch_default { "true" } else { "false" },
    )
    .await?;

    Ok(s)
}

// ---- 存储占用 ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub data_dir: String,
    pub db_bytes: u64,
    pub snapshot_count: i64,
    pub snapshot_bytes: u64,
    pub run_count: i64,
}

fn dir_stats(dir: &std::path::Path) -> (i64, u64) {
    let mut count = 0i64;
    let mut bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                count += 1;
                bytes += meta.len();
            }
        }
    }
    (count, bytes)
}

pub async fn storage_stats(db: &Db) -> AppResult<StorageStats> {
    let dir = storage::data_dir();
    let db_path = dir.join("workscene.db");
    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    // WAL 模式下游标内容可能还在 -wal 里，一并计入更接近真实占用
    let wal_bytes = std::fs::metadata(dir.join("workscene.db-wal"))
        .map(|m| m.len())
        .unwrap_or(0);

    let (snapshot_count, snapshot_bytes) = dir_stats(&storage::snapshot_dir());
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM restore_runs")
        .fetch_one(&db.pool)
        .await?;

    Ok(StorageStats {
        data_dir: dir.to_string_lossy().to_string(),
        db_bytes: db_bytes + wal_bytes,
        snapshot_count,
        snapshot_bytes,
        run_count,
    })
}

/// 按当前保留设置为所有工作空间清理旧自动快照
pub async fn prune_all(db: &Db) -> AppResult<usize> {
    let keep = get(db).await?.auto_keep.max(1) as usize;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM workspaces")
        .fetch_all(&db.pool)
        .await?;

    let mut removed = 0usize;
    for id in ids {
        removed += super::snapshots::prune(db, &id, keep).await.unwrap_or(0);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::in_memory;

    #[tokio::test]
    async fn defaults_are_sane() {
        let db = in_memory().await.unwrap();
        let s = get(&db).await.unwrap();
        assert!(s.auto_snapshot_enabled);
        assert_eq!(s.auto_keep, 20);
        assert_eq!(s.auto_min_interval_secs, 180);
        assert!(s.restore_relaunch_default);
    }

    #[tokio::test]
    async fn set_roundtrips_and_clamps() {
        let db = in_memory().await.unwrap();
        let saved = set(
            &db,
            &AppSettings {
                auto_snapshot_enabled: false,
                auto_keep: 0,          // 应被夹到 1
                auto_min_interval_secs: 1, // 应被夹到 10
                restore_relaunch_default: false,
                autostart: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(saved.auto_keep, 1);
        assert_eq!(saved.auto_min_interval_secs, 10);

        let round = get(&db).await.unwrap();
        assert!(!round.auto_snapshot_enabled);
        assert_eq!(round.auto_keep, 1);
        assert!(!round.restore_relaunch_default);
    }
}
