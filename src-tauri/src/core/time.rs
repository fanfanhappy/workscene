use std::time::{SystemTime, UNIX_EPOCH};

/// 全库统一时间：UTC Unix 毫秒（文档附录 B 约定）
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 自动快照循环的正常间隔是 60 秒；墙钟一次跳变超过 2 分钟，
/// 基本可以判定为系统休眠/唤醒（或用户手动改时间）。
/// 这里用于唤醒后暂停自动快照，避免把驱动重排后的窗口坐标当成新现场。
pub const RESUME_GAP_MS: i64 = 120_000;

pub fn looks_like_resume(last_wall_ms: i64, now_wall_ms: i64) -> bool {
    now_wall_ms.saturating_sub(last_wall_ms) > RESUME_GAP_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sleep_gap_but_not_normal_tick() {
        assert!(!looks_like_resume(1_000, 61_000));
        assert!(looks_like_resume(1_000, 1_000 + RESUME_GAP_MS + 1));
        // 墙钟被往回调时不能误判为休眠
        assert!(!looks_like_resume(200_000, 100_000));
    }
}
