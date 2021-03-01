use serde::{Deserialize, Serialize};
use tauri::api::rpc::format_callback_result;

use crate::awsl;
use crate::cal::{
    error::TLCResult,
    preprocess::{FilterMethod, InterpMethod},
    solve::IterationMethod,
};

/// body数据类型
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Value {
    String(String),
    Uint(usize),
    Float(f32),
    UintVec(Vec<usize>),
    FloatVec(Vec<f32>),
    IntPairVec(Vec<(i32, i32)>),
    Interp(InterpMethod),
    Filter(FilterMethod),
    Iteration(IterationMethod),
}

#[derive(Debug, Deserialize)]
pub struct Request {
    /// as url
    pub cmd: String,
    pub body: Option<Value>,
    pub callback: String,
    pub error: String,
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