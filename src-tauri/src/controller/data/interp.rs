use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum InterpMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear { shape: (usize, usize) },
    BilinearExtra { shape: (usize, usize) },
}

impl Default for InterpMethod {
    fn default() -> Self {
        InterpMethod::Horizontal
    }
}
