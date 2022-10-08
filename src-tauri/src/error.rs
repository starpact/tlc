use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("setting({0}) id not found")]
    SettingIdNotFound(i64),
}
