mod interp;
mod raw;

use std::path::PathBuf;

use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

pub use interp::{interp, InterpId, InterpMethod, Interpolator};
pub use raw::read_daq;

pub struct DaqData {
    daq_meta: DaqMeta,
    daq_raw: ArcArray2<f64>,
    interpolator: Option<Interpolator>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DaqId {
    pub daq_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct DaqMeta {
    pub nrows: usize,
    pub ncols: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Hash)]
pub struct Thermocouple {
    /// Column index of this thermocouple in the DAQ file.
    pub column_index: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    pub position: (i32, i32),
}

impl DaqData {
    pub fn new(daq_meta: DaqMeta, daq_raw: ArcArray2<f64>) -> DaqData {
        DaqData {
            daq_meta,
            daq_raw,
            interpolator: None,
        }
    }

    pub fn daq_meta(&self) -> DaqMeta {
        self.daq_meta
    }

    pub fn daq_raw(&self) -> ArcArray2<f64> {
        self.daq_raw.clone()
    }

    pub fn interpolator(&self) -> Option<&Interpolator> {
        self.interpolator.as_ref()
    }

    pub fn set_interpolator(&mut self, interpolator: Option<Interpolator>) {
        self.interpolator = interpolator;
    }
}
