#![cfg_attr(test, feature(test, array_windows, portable_simd))]
#![allow(clippy::too_many_arguments)]

mod daq;
mod postproc;
mod solve;
mod state;
#[cfg(test)]
mod tests;
mod util;
mod video;

pub use daq::{InterpMethod, Thermocouple};
pub use salsa::ParallelDatabase;
pub use solve::{IterMethod, PhysicalParam};
use state::CalNumId;
pub use state::{decode_frame_base64, Database, NuData};
pub use video::FilterMethod;

#[salsa::jar(db = Db)]
pub struct Jar(
    // input
    video::VideoPathId,
    daq::DaqPathId,
    daq::ThermocouplesId,
    // interned
    video::AreaId,
    video::FilterMethodId,
    daq::InterpMethodId,
    solve::PhysicalParamId,
    solve::IterMethodId,
    state::StartIndexId,
    // tracked
    video::VideoDataId,
    video::Green2Id,
    video::PointId,
    video::GmaxFrameIndexesId,
    daq::DaqDataId,
    daq::InterpolatorId,
    state::CalNumId,
    solve::Nu2Id,
    // tracked function
    video::read_video,
    video::decode_all,
    video::filter_detect_peak,
    video::filter_point,
    daq::read_daq,
    daq::make_interpolator,
    state::eval_cal_num,
    solve::solve_nu,
);

pub trait Db: salsa::DbWithJar<Jar> {}

pub fn init() {
    video::init();
    util::log::init();
}
