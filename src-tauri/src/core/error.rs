use serde::{Serialize, Serializer};

/// 领域错误：每个错误都带可读的中文说明，可直接展示在恢复报告与 UI 上。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文件或目录操作失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("数据格式错误：{0}")]
    Json(#[from] serde_json::Error),

    #[error("数据库操作失败：{0}")]
    Db(#[from] sqlx::Error),

    #[error("{0}")]
    Message(String),

    #[error("未找到快照：{0}")]
    SceneNotFound(String),
}

impl AppError {
    pub fn msg(msg: impl Into<String>) -> Self {
        AppError::Message(msg.into())
    }
}

impl From<String> for AppError {
    fn from(v: String) -> Self {
        AppError::Message(v)
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
