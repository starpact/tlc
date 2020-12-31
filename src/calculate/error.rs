use thiserror::Error;

#[derive(Error, Debug)]
pub enum TLCError {
    #[error("configuration io error: {0}")]
    ConfigIOError(String),

    #[error("wrong json format: {0}")]
    ConfigFormatError(#[from] serde_json::error::Error),

    #[error("video io error: {0}")]
    VideoIOError(String),

    #[error("video error: {0}")]
    VideoError(#[from] ffmpeg_next::util::error::Error),

    #[error("daq io error: {0}")]
    DAQIOError(String),

    #[error("failed to create directory: {0}")]
    CreateDirFailedError(std::io::Error),

    #[error("Nu io error: {0}")]
    NuIOError(String),

    #[error("fail to plot: {0}")]
    PlotError(String),

    #[error("unknown error: {0}")]
    UnKnown(String),
}

pub type TLCResult<T> = Result<T, TLCError>;

impl From<calamine::XlsxError> for TLCError {
    fn from(err: calamine::XlsxError) -> Self {
        TLCError::DAQIOError(err.to_string())
    }
}

impl From<csv::Error> for TLCError {
    fn from(err: csv::Error) -> Self {
        TLCError::DAQIOError(err.to_string())
    }
}
