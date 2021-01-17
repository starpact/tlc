use thiserror::Error;

/// Rust的std::io::Error不会包含错误路径，需要自己封装
///
/// >The number one problem with std::io::Error is that, when a file-system operation fails,
/// you don’t know which path it has failed for! This is understandable — Rust is a systems language,
/// so it shouldn’t add much fat over what OS provides natively. OS returns an integer return code,
/// and coupling that with a heap-allocated PathBuf could be an unacceptable overhead!
#[derive(Error, Debug)]
pub enum TLCError {
    #[error("配置文件读取失败：{raw_err}\n请检查配置文件路径：{context}")]
    ConfigIOError { raw_err: String, context: String },

    #[error("配置文件错误: {0}")]
    ConfigError(#[from] serde_json::error::Error),

    #[error("请检查视频文件路径：{0}")]
    VideoIOError(String),

    #[error("视频文件错误: {raw_err}\n{context}")]
    VideoError { raw_err: String, context: String },

    #[error("数据采集文件读取失败：{raw_err}\n请检查数据采集文件路径：{context}")]
    DAQIOError { raw_err: String, context: String },

    #[error("数据采集文件解析错误：{raw_err}\n请检查数据采集文件：{context}")]
    DAQError { raw_err: String, context: String },

    #[error("创建保存结果的子目录失败: {raw_err}\n请检查保存根路径：{context}")]
    CreateDirError { raw_err: String, context: String },

    #[error("Nu矩阵保存失败：{raw_err}\n请检查文件是否被占用以及保存路径：{context}")]
    NuSaveError { raw_err: String, context: String },

    #[error("Nu矩阵读取失败：{raw_err}\n请检查Nu矩阵文件：{context}")]
    NuReadError { raw_err: String, context: String },

    #[error("画图失败: {0}")]
    PlotError(String),

    #[error("未知错误: {0}")]
    UnKnown(String),
}

pub type TLCResult<T> = Result<T, TLCError>;

#[macro_export]
macro_rules! err {
    ($member:tt, $context:expr $(,)*) => {
        $crate::error::TLCError::$member(format!("{:?}", $context))
    };

    ($member:tt, $raw_err:expr, $context:expr $(,)*) => {
        $crate::error::TLCError::$member {
            raw_err: format!("{:?}", $raw_err),
            context: format!("{:?}", $context),
        }
    };
}
