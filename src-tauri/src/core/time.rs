use std::time::{SystemTime, UNIX_EPOCH};

/// 全库统一时间：UTC Unix 毫秒（文档附录 B 约定）
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
