#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]
#![allow(clippy::too_many_arguments)]

mod daq;
mod post_processing;
mod solve;
mod state;
mod util;
mod video;

use daq::{
    make_interpolator, read_daq, DaqDataId, DaqPathId, InterpMethodId, InterpolatorId,
    ThermocouplesId,
};
pub use daq::{InterpMethod, Thermocouple};
use solve::{IterMethodId, NuDataId, PyhsicalParamId};
pub use solve::{IterationMethod, PhysicalParam};
pub use state::{decode_frame, Database};
use state::{eval_cal_num, CalNumId, StartIndexId};
use video::{
    decode_all, filter_detect_peak, filter_point, read_video, AreaId, FilterMethodId,
    GmaxFrameIndexesId, Green2Id, PointId, VideoDataId, VideoPathId,
};
pub use video::{FilterMethod, VideoMeta};

#[salsa::jar(db = Db)]
pub struct Jar(
    // input
    VideoPathId,
    DaqPathId,
    ThermocouplesId,
    // interned
    AreaId,
    FilterMethodId,
    InterpMethodId,
    PyhsicalParamId,
    IterMethodId,
    StartIndexId,
    // tracked
    VideoDataId,
    Green2Id,
    PointId,
    GmaxFrameIndexesId,
    DaqDataId,
    InterpolatorId,
    CalNumId,
    NuDataId,
    // tracked function
    read_video,
    decode_all,
    filter_detect_peak,
    filter_point,
    read_daq,
    make_interpolator,
    eval_cal_num,
    solve::solve_nu,
);

pub trait Db: salsa::DbWithJar<Jar> {}

pub fn init() {
    video::init();
    util::log::init();
}
