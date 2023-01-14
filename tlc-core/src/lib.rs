#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]

mod daq;
mod main_loop;
mod post_processing;
pub mod request;
mod setting;
mod solve;
mod state;
mod util;
mod video;

use std::path::PathBuf;

use ndarray::ArcArray2;
use salsa::{DebugWithDb, Snapshot};
use tracing::debug;

pub use daq::{DaqMeta, InterpMethod, Thermocouple};
pub use main_loop::run;
pub use setting::StartIndex;
pub use solve::{IterationMethod, PhysicalParam};
pub use util::{log, progress_bar::Progress};
pub use video::{FilterMethod, VideoMeta};

#[salsa::jar(db = Db)]
pub struct Jar(
    // input
    video::VideoPathId,
    daq::DaqPathId,
    daq::Thermocouples,
    // interned
    video::StartFrameId,
    video::Area,
    video::FilterMethodId,
    daq::StartRowId,
    daq::InterpMethodId,
    solve::PyhsicalParamId,
    solve::IterationMethodId,
    // tracked
    video::VideoDataId,
    video::Green2,
    video::FilteredGreen2,
    daq::DaqDataId,
    daq::InterpolatorId,
    CalNumId,
    // tracked function
    video::_read_video,
    video::_decode_all,
    daq::_read_daq,
    daq::_interp,
    _get_cal_num,
);

#[derive(Default)]
#[salsa::db(Jar)]
pub struct Database {
    storage: salsa::Storage<Self>,
    video_path_id: Option<video::VideoPathId>,
    daq_path_id: Option<daq::DaqPathId>,
}

pub trait Db: salsa::DbWithJar<Jar> {}

impl salsa::Database for Database {
    fn salsa_event(&self, event: salsa::Event) {
        match event.kind {
            salsa::EventKind::WillCheckCancellation => {}
            _ => debug!("salsa_event: {:?}", event.debug(self)),
        }
    }
}

impl salsa::ParallelDatabase for Database {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        Snapshot::new(Self {
            storage: self.storage.snapshot(),
            video_path_id: self.video_path_id,
            daq_path_id: self.daq_path_id,
        })
    }
}

impl Db for Database {}

impl Database {
    pub fn get_video_path(&self) -> Option<PathBuf> {
        Some(self.video_path_id?.path(self))
    }

    pub fn set_video_path(&mut self, video_path: PathBuf) {
        match self.video_path_id {
            Some(video_path_id) => {
                video_path_id.set_path(self).to(video_path);
            }
            None => self.video_path_id = Some(video::VideoPathId::new(self, video_path)),
        }
    }

    pub fn get_video_nframes(&self) -> Result<usize, String> {
        Ok(self.video_data_id()?.packets(self).0.len())
    }

    pub fn get_video_frame_rate(&self) -> Result<usize, String> {
        Ok(self.video_data_id()?.frame_rate(self))
    }

    pub fn get_video_shape(&self) -> Result<(u32, u32), String> {
        Ok(self.video_data_id()?.shape(self))
    }

    pub fn decode_frame(&self, frame_index: usize) -> Result<String, String> {
        let video_data_id = self.video_data_id()?;
        let frame_base64 = video::_decode_frame_base64(self, video_data_id, frame_index)
            .map_err(|e| e.to_string())?;
        Ok(frame_base64)
    }

    pub fn get_daq_path(&self) -> Option<PathBuf> {
        Some(self.daq_path_id?.path(self))
    }

    pub fn set_daq_path(&mut self, daq_path: PathBuf) {
        match self.daq_path_id {
            Some(daq_path_id) => {
                daq_path_id.set_path(self).to(daq_path);
            }
            None => self.daq_path_id = Some(daq::DaqPathId::new(self, daq_path)),
        }
    }

    pub fn get_daq_data(&self) -> Result<ArcArray2<f64>, String> {
        let daq_path_id = self.daq_path_id.ok_or("daq path unset".to_owned())?;
        let daq_data = daq::_read_daq(self, daq_path_id)?.data(self).0;
        Ok(daq_data)
    }

    fn video_data_id(&self) -> Result<video::VideoDataId, String> {
        let video_path_id = self.video_path_id.ok_or("video path unset".to_owned())?;
        video::_read_video(self, video_path_id)
    }
}

#[salsa::tracked]
pub(crate) struct CalNumId {
    pub cal_num: usize,
}

#[salsa::tracked]
pub(crate) fn _get_cal_num(
    db: &dyn crate::Db,
    video_data_id: video::VideoDataId,
    daq_data_id: daq::DaqDataId,
    start_frame_id: video::StartFrameId,
    start_row_id: daq::StartRowId,
) -> CalNumId {
    let nframes = video_data_id.packets(db).0.len();
    let nrows = daq_data_id.data(db).0.nrows();
    let start_frame = start_frame_id.start_frame(db);
    let start_row = start_row_id.start_row(db);
    CalNumId::new(db, (nframes - start_frame).min(nrows - start_row))
}
