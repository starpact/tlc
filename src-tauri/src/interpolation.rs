use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum InterpMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear(usize, usize),
    BilinearExtra(usize, usize),
}
