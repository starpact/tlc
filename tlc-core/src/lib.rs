#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]
#![allow(clippy::too_many_arguments)]

mod daq;
mod post_processing;
mod solve;
mod state;
#[cfg(test)]
mod tests;
mod util;
mod video;

use daq::{
    make_interpolator, read_daq, DaqDataId, DaqPathId, InterpMethodId, InterpolatorId,
    ThermocouplesId,
};
pub use daq::{InterpMethod, Thermocouple};
use post_processing::TruncId;
pub use solve::{IterMethod, PhysicalParam};
use solve::{IterMethodId, Nu2Id, PhysicalParamId};
pub use state::{decode_frame_base64, Database, NuData};
use state::{eval_cal_num, CalNumId, NameId, SaveRootDirId, StartIndexId};
pub use video::FilterMethod;
use video::{
    decode_all, filter_detect_peak, filter_point, read_video, AreaId, FilterMethodId,
    GmaxFrameIndexesId, Green2Id, PointId, VideoDataId, VideoPathId,
};

#[salsa::jar(db = Db)]
pub struct Jar(
    // input
    VideoPathId,
    DaqPathId,
    ThermocouplesId,
    TruncId,
    // interned
    NameId,
    SaveRootDirId,
    AreaId,
    FilterMethodId,
    InterpMethodId,
    PhysicalParamId,
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
    Nu2Id,
    // tracked function
    read_video,
    decode_all,
    filter_detect_peak,
    filter_point,
    read_daq,
    make_interpolator,
    eval_cal_num,
    solve::solve_nu,
    post_processing::save_nu_matrix,
    post_processing::save_setting,
    post_processing::save_nu_plot,
);

pub trait Db: salsa::DbWithJar<Jar> {}

pub fn init() {
    video::init();
    util::log::init();
}
