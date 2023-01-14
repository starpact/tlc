mod interp;
mod raw;
#[cfg(test)]
mod test;

use std::path::PathBuf;

use ndarray::ArcArray2;

use crate::util::impl_eq_always_false;
pub use interp::{interp, InterpId, InterpMethod, Interpolator};
pub use raw::{read_daq, DaqData, DaqId, DaqMeta, Thermocouple};

#[salsa::input]
pub(crate) struct DaqPathId {
    pub path: PathBuf,
}

#[salsa::tracked]
pub(crate) struct DaqDataId {
    pub data: DaqData1,
}

#[derive(Debug, Clone)]
pub(crate) struct DaqData1(pub ArcArray2<f64>);

#[salsa::tracked]
pub(crate) struct InterpolatorId {
    pub interpolater: Interpolator,
}

impl_eq_always_false!(DaqData1, Interpolator);

#[salsa::interned]
pub(crate) struct StartRowId {
    pub start_row: usize,
}

#[salsa::input]
pub(crate) struct Thermocouples {
    pub thermocouples: Vec<Thermocouple>,
}

#[salsa::interned]
pub(crate) struct InterpMethodId {
    pub interp_method: InterpMethod,
}

/// See `read_video`.
#[salsa::tracked]
pub(crate) fn _read_daq(db: &dyn crate::Db, daq_path_id: DaqPathId) -> Result<DaqDataId, String> {
    let daq_path = daq_path_id.path(db);
    let daq_data = read_daq(daq_path).map_err(|e| e.to_string())?;
    Ok(DaqDataId::new(db, DaqData1(daq_data.into_shared())))
}

#[salsa::tracked]
pub(crate) fn _interp(
    db: &dyn crate::Db,
    daq_path_id: DaqPathId,
    interp_method_id: InterpMethodId,
) -> Result<InterpolatorId, String> {
    let _daq_data = _read_daq(db, daq_path_id)?.data(db);
    let _interp_method = interp_method_id.interp_method(db);
    todo!()
}
