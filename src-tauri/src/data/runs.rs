use crate::core::models::RestoreReport;
use crate::core::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub workspace_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub status: String,
    pub trigger_type: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub total_steps: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
}

/// 整份报告落库：一次 run + 逐条 step（带 seq 与 error_code，可聚合统计）
pub async fn save(
    db: &Db,
    report: &RestoreReport,
    workspace_id: Option<&str>,
    snapshot_id: Option<&str>,
    trigger: &str,
) -> AppResult<()> {
    let status = if report.failed > 0 {
        if report.succeeded > 0 {
            "partial"
        } else {
            "failed"
        }
    } else {
        "completed"
    };

    let mut tx = db.pool.begin().await?;

    sqlx::query(
        "INSERT INTO restore_runs (id, workspace_id, snapshot_id, status, trigger_type,
            started_at, finished_at, total_steps, succeeded, failed, skipped)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&report.run_id)
    .bind(workspace_id)
    .bind(snapshot_id)
    .bind(status)
    .bind(trigger)
    .bind(report.started_at)
    .bind(report.finished_at)
    .bind(report.total_steps as i64)
    .bind(report.succeeded as i64)
    .bind(report.failed as i64)
    .bind(report.skipped as i64)
    .execute(&mut *tx)
    .await?;

    for (seq, step) in report.steps.iter().enumerate() {
        sqlx::query(
            "INSERT INTO restore_steps (id, run_id, seq, step_type, target_name, status,
                error_code, message, match_note, duration_ms, delta_x, delta_y, state_restored)
             VALUES (?, ?, ?, 'window', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("{}-{}", report.run_id, seq))
        .bind(&report.run_id)
        .bind(seq as i64)
        .bind(&step.target_name)
        .bind(format!("{:?}", step.status).to_lowercase())
        .bind(&step.code)
        .bind(&step.message)
        .bind(&step.match_note)
        .bind(step.duration_ms)
        .bind(step.delta_x.map(|v| v as i64))
        .bind(step.delta_y.map(|v| v as i64))
        .bind(if step.state_restored { 1 } else { 0 })
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn list(db: &Db, limit: usize) -> AppResult<Vec<RunSummary>> {
    let rows = sqlx::query(
        "SELECT id, workspace_id, snapshot_id, status, trigger_type, started_at, finished_at,
                total_steps, succeeded, failed, skipped
         FROM restore_runs ORDER BY started_at DESC LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RunSummary {
            id: r.get("id"),
            workspace_id: r.get("workspace_id"),
            snapshot_id: r.get("snapshot_id"),
            status: r.get("status"),
            trigger_type: r.get("trigger_type"),
            started_at: r.get("started_at"),
            finished_at: r.get("finished_at"),
            total_steps: r.get("total_steps"),
            succeeded: r.get("succeeded"),
            failed: r.get("failed"),
            skipped: r.get("skipped"),
        })
        .collect())
}

/// 成功率基线用：按错误码聚合（docs/RESTORE_CONTRACT.md §3）
pub async fn error_code_stats(db: &Db) -> AppResult<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT COALESCE(error_code, 'ok') AS code, COUNT(*) AS n
         FROM restore_steps GROUP BY code ORDER BY n DESC",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("code"), r.get::<i64, _>("n")))
        .collect())
}
