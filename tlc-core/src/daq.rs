mod interp;
pub(crate) mod io;

use std::path::PathBuf;

use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

use crate::{state::StartIndexId, util::impl_eq_always_false, video::AreaId, CalNumId};
pub use interp::{InterpMethod, Interpolator};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct DaqMeta {
    pub nrows: usize,
    pub ncols: usize,
}

#[salsa::input]
pub(crate) struct DaqPathId {
    #[return_ref]
    pub path: PathBuf,
}

#[salsa::tracked]
pub(crate) struct DaqDataId {
    pub data: DaqData,
}

#[derive(Debug, Clone)]
pub(crate) struct DaqData(pub ArcArray2<f64>);

#[salsa::tracked]
pub(crate) struct InterpolatorId {
    pub interpolater: Interpolator,
}

impl_eq_always_false!(DaqData, Interpolator);

#[salsa::input]
pub(crate) struct ThermocouplesId {
    #[return_ref]
    pub thermocouples: Vec<Thermocouple>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct Thermocouple {
    /// Column index of this thermocouple in the DAQ file.
    pub column_index: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    pub position: (i32, i32),
}

#[salsa::interned]
pub(crate) struct InterpMethodId {
    pub interp_method: InterpMethod,
}

/// See `read_video`.
#[salsa::tracked]
pub(crate) fn read_daq(db: &dyn crate::Db, daq_path_id: DaqPathId) -> Result<DaqDataId, String> {
    let daq_path = daq_path_id.path(db);
    let daq_data = io::read_daq(daq_path).map_err(|e| e.to_string())?;
    Ok(DaqDataId::new(db, DaqData(daq_data.into_shared())))
}

#[salsa::tracked]
pub(crate) fn make_interpolator(
    db: &dyn crate::Db,
    daq_data_id: DaqDataId,
    start_index_id: StartIndexId,
    cal_num_id: CalNumId,
    area_id: AreaId,
    thermocouples_id: ThermocouplesId,
    interp_method_id: InterpMethodId,
) -> InterpolatorId {
    let daq_data = daq_data_id.data(db).0;
    let start_row = start_index_id.start_row(db);
    let cal_num = cal_num_id.cal_num(db);
    let area = area_id.area(db);
    let interp_method = interp_method_id.interp_method(db);
    let thermocouples = thermocouples_id.thermocouples(db);
    let interpolator = Interpolator::new(
        start_row,
        cal_num,
        area,
        interp_method,
        thermocouples,
        daq_data.view(),
    );
    InterpolatorId::new(db, interpolator)
}
