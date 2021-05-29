use serde::{Deserialize, Serialize};
use tauri::api::rpc::format_callback_result;

use crate::awsl;
use crate::cal::preprocess::{FilterMethod, InterpMethod};
use crate::cal::{error::TLCResult, solve::IterationMethod, Thermocouple};

/// body数据类型
#[derive(Debug, Deserialize)]
pub enum Value {
    Nothing,
    String(String),
    Uint(usize),
    Float(f32),
    UintVec(Vec<usize>),
    FloatVec(Vec<f32>),
    Thermocouples(Vec<Thermocouple>),
    Interp(InterpMethod),
    Filter(FilterMethod),
    Iteration(IterationMethod),
}

#[derive(Debug, Deserialize)]
pub struct Request {
    /// as url
    pub cmd: String,
    /// js数据类型映射到rust
    #[serde(default)]
    pub body: Value,
    /// then
    pub callback: String,
    /// catch
    pub error: String,
}

impl Default for Value {
    fn default() -> Self {
        Self::Nothing
    }
}

impl Request {
    pub fn format_callback<T: Serialize, E: ToString>(
        result: Result<T, E>,
        callback: String,
        error: String,
    ) -> TLCResult<String> {
        format_callback_result(result.map_err(|err| err.to_string()), callback, error)
            .map_err(|err| awsl!(err))
    }
}
