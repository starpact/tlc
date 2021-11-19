use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum IterationMethod {
    NewtonTangent { h0: f32, max_iter_num: usize },
    NewtonDown { h0: f32, max_iter_num: usize },
}

impl Default for IterationMethod {
    fn default() -> Self {
        Self::NewtonTangent {
            h0: 50.0,
            max_iter_num: 10,
        }
    }
}
