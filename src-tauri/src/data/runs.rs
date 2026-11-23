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
    let cancelled = report
        .steps
        .iter()
        .any(|s| s.code.as_deref() == Some("cancelled"));

    let status = if cancelled {
        "cancelled"
    } else if report.failed > 0 {
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
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("{}-{}", report.run_id, seq))
        .bind(&report.run_id)
        .bind(seq as i64)
        // 之前这里写死了 'window'，导致资源步骤在记录里被标成窗口步骤
        .bind(&step.step_type)
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

/// 单条恢复步骤（从库里回读，用于"恢复记录"详情）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepRow {
    pub seq: i64,
    pub step_type: String,
    pub target_name: String,
    pub status: String,
    pub error_code: Option<String>,
    pub message: String,
    pub match_note: Option<String>,
    pub duration_ms: Option<i64>,
    pub delta_x: Option<i64>,
    pub delta_y: Option<i64>,
    pub state_restored: bool,
}

pub async fn steps(db: &Db, run_id: &str) -> AppResult<Vec<RunStepRow>> {
    let rows = sqlx::query(
        "SELECT seq, step_type, target_name, status, error_code, message, match_note,
                duration_ms, delta_x, delta_y, state_restored
         FROM restore_steps WHERE run_id = ? ORDER BY seq",
    )
    .bind(run_id)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RunStepRow {
            seq: r.get("seq"),
            step_type: r.get("step_type"),
            target_name: r.get("target_name"),
            status: r.get("status"),
            error_code: r.get("error_code"),
            message: r.get("message"),
            match_note: r.get("match_note"),
            duration_ms: r.get("duration_ms"),
            delta_x: r.get("delta_x"),
            delta_y: r.get("delta_y"),
            state_restored: r.get::<i64, _>("state_restored") != 0,
        })
        .collect())
}

/// 清空全部恢复执行记录（外键 ON DELETE CASCADE 自动清理 restore_steps）
pub async fn clear_all(db: &Db) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM restore_runs")
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{RestoreStepResult, StepStatus};

    fn report() -> RestoreReport {
        RestoreReport {
            run_id: "run-test-1".into(),
            scene_id: "scene-1".into(),
            scene_name: "测试现场".into(),
            started_at: 1_700_000_000_000,
            finished_at: 1_700_000_001_000,
            total_steps: 2,
            succeeded: 1,
            failed: 1,
            skipped: 0,
            steps: vec![
                RestoreStepResult {
                    step_type: "window".into(),
                    target_name: "窗口 A".into(),
                    status: StepStatus::Success,
                    message: "已恢复".into(),
                    match_note: "0.95".into(),
                    duration_ms: 120,
                    delta_x: Some(0),
                    delta_y: Some(0),
                    state_restored: true,
                    code: None,
                },
                RestoreStepResult {
                    step_type: "resource".into(),
                    target_name: "README".into(),
                    status: StepStatus::Failed,
                    message: "无法打开".into(),
                    match_note: "D:/x".into(),
                    duration_ms: 5,
                    delta_x: None,
                    delta_y: None,
                    state_restored: true,
                    code: Some("resource_open_failed".into()),
                },
            ],
            display_notes: vec![],
            pre_restore_scene_id: None,
        }
    }

    #[tokio::test]
    async fn save_then_read_back_run_and_steps() {
        let db = crate::data::in_memory().await.unwrap();
        save(&db, &report(), None, None, "test").await.unwrap();

        let runs = list(&db, 10).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "partial", "有失败也有成功 -> partial");
        assert_eq!(runs[0].total_steps, 2);

        let steps = steps(&db, "run-test-1").await.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].seq, 0);
        assert_eq!(steps[1].step_type, "resource");
        assert_eq!(steps[1].error_code.as_deref(), Some("resource_open_failed"));
        assert!(steps[0].state_restored);
    }

    #[tokio::test]
    async fn error_code_stats_group_by_code() {
        let db = crate::data::in_memory().await.unwrap();
        save(&db, &report(), None, None, "test").await.unwrap();

        let stats = error_code_stats(&db).await.unwrap();
        let ok = stats.iter().find(|(c, _)| c == "ok").map(|(_, n)| *n);
        let failed = stats
            .iter()
            .find(|(c, _)| c == "resource_open_failed")
            .map(|(_, n)| *n);
        assert_eq!(ok, Some(1));
        assert_eq!(failed, Some(1));
    }
}
