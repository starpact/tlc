#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]
#![allow(clippy::too_many_arguments)]

mod daq;
mod post_processing;
mod solve;
mod util;
mod video;

use std::path::PathBuf;

use ndarray::{ArcArray2, Array2};
use salsa::{DebugWithDb, Snapshot};
use tracing::debug;

use daq::{
    make_interpolator, read_daq, DaqDataId, DaqPathId, InterpMethodId, InterpolatorId,
    ThermocouplesId,
};
pub use daq::{InterpMethod, Thermocouple};
pub use solve::{IterationMethod, PhysicalParam};
use solve::{IterationMethodId, NuDataId, PyhsicalParamId};
use video::{
    decode_all, decode_frame_base64, filter_detect_peak, filter_point, read_video, AreaId,
    FilterMethodId, GmaxFrameIndexesId, Green2Id, PointId, VideoDataId, VideoPathId,
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
    IterationMethodId,
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
    get_cal_num,
    solve::solve_nu,
);

#[derive(Default)]
#[salsa::db(Jar)]
pub struct Database {
    storage: salsa::Storage<Self>,
    name: Option<String>,
    save_root_dir: Option<PathBuf>,
    video_path_id: Option<VideoPathId>,
    daq_path_id: Option<DaqPathId>,
    start_index_id: Option<StartIndexId>,
    area_id: Option<AreaId>,
    thermocouples_id: Option<ThermocouplesId>,
    filter_method_id: Option<FilterMethodId>,
    interp_method_id: Option<InterpMethodId>,
    physical_param_id: Option<solve::PyhsicalParamId>,
    iteration_method_id: Option<solve::IterationMethodId>,
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
            name: self.name.clone(),
            save_root_dir: self.save_root_dir.clone(),
            video_path_id: self.video_path_id,
            daq_path_id: self.daq_path_id,
            start_index_id: self.start_index_id,
            area_id: self.area_id,
            thermocouples_id: self.thermocouples_id,
            filter_method_id: self.filter_method_id,
            interp_method_id: self.interp_method_id,
            physical_param_id: self.physical_param_id,
            iteration_method_id: self.iteration_method_id,
        })
    }
}

impl Db for Database {}

impl Database {
    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    pub fn get_save_root_dir(&self) -> Option<PathBuf> {
        self.save_root_dir.clone()
    }

    pub fn set_save_root_dir(&mut self, save_root_dir: PathBuf) {
        self.save_root_dir = Some(save_root_dir);
    }

    pub fn get_video_path(&self) -> Option<PathBuf> {
        Some(self.video_path_id?.path(self))
    }

    pub fn set_video_path(&mut self, video_path: PathBuf) {
        self.start_index_id = None;
        match self.video_path_id {
            Some(video_path_id) => {
                video_path_id.set_path(self).to(video_path);
            }
            None => self.video_path_id = Some(VideoPathId::new(self, video_path)),
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
        let frame_base64 =
            decode_frame_base64(self, video_data_id, frame_index).map_err(|e| e.to_string())?;
        Ok(frame_base64)
    }

    pub fn get_daq_path(&self) -> Option<PathBuf> {
        Some(self.daq_path_id?.path(self))
    }

    pub fn set_daq_path(&mut self, daq_path: PathBuf) {
        self.start_index_id = None;
        match self.daq_path_id {
            Some(daq_path_id) => {
                daq_path_id.set_path(self).to(daq_path);
            }
            None => self.daq_path_id = Some(DaqPathId::new(self, daq_path)),
        }
    }

    pub fn get_daq_data(&self) -> Result<ArcArray2<f64>, String> {
        let daq_path_id = self.daq_path_id.ok_or("daq path unset".to_owned())?;
        let daq_data = read_daq(self, daq_path_id)?.data(self).0;
        Ok(daq_data)
    }

    pub fn synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<(), String> {
        let nframes = self.get_video_nframes()?;
        if start_frame >= nframes {
            return Err(format!(
                "frame_index({start_frame}) out of range({nframes})"
            ));
        }
        let nrows = self.get_daq_data()?.nrows();
        if start_row >= nrows {
            return Err(format!("row_index({start_row}) out of range({nrows})"));
        }
        self.start_index_id = Some(StartIndexId::new(self, start_frame, start_row));
        Ok(())
    }

    pub fn get_start_frame(&self) -> Result<usize, String> {
        Ok(self.start_index_id()?.start_frame(self))
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<(), String> {
        let nframes = self.get_video_nframes()?;
        if start_frame >= nframes {
            return Err(format!(
                "frame_index({start_frame}) out of range({nframes})"
            ));
        }
        let nrows = self.get_daq_data()?.nrows();
        let old_start_frame = self.get_start_frame()?;
        let old_start_row = self.get_start_row()?;
        if old_start_row + start_frame < old_start_frame {
            return Err("invalid start_frame".to_owned());
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            return Err(format!("row_index({start_row}) out of range({nrows})"));
        }
        self.start_index_id = Some(StartIndexId::new(self, start_frame, start_row));
        Ok(())
    }

    pub fn get_start_row(&self) -> Result<usize, String> {
        Ok(self.start_index_id()?.start_row(self))
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<(), String> {
        let nrows = self.get_daq_data()?.nrows();
        if start_row >= nrows {
            return Err(format!("row_index({start_row}) out of range({nrows})"));
        }
        let nframes = self.get_video_nframes()?;
        let old_start_frame = self.get_start_frame()?;
        let old_start_row = self.get_start_row()?;
        if old_start_frame + start_row < old_start_row {
            return Err("invalid start_frame".to_owned());
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            return Err(format!(
                "frames_index({start_frame}) out of range({nframes})"
            ));
        }
        self.start_index_id = Some(StartIndexId::new(self, start_frame, start_row));
        Ok(())
    }

    pub fn get_area(&self) -> Option<(u32, u32, u32, u32)> {
        Some(self.area_id?.area(self))
    }

    pub fn set_area(&mut self, area: (u32, u32, u32, u32)) -> Result<(), String> {
        let (h, w) = self.get_video_shape()?;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            return Err(format!(
                "area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})"
            ));
        }
        if tl_y + cal_h > h {
            return Err(format!(
                "area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})"
            ));
        }
        self.area_id = Some(AreaId::new(self, area));
        Ok(())
    }

    pub fn get_filter_method(&self) -> Option<FilterMethod> {
        Some(self.filter_method_id?.filter_method(self))
    }

    pub fn set_filter_method(&mut self, filter_method: FilterMethod) {
        self.filter_method_id = Some(FilterMethodId::new(self, filter_method));
    }

    pub fn filter_point(&self, point: (usize, usize)) -> Result<Vec<u8>, String> {
        let video_data_id = self.video_data_id()?;
        let daq_data_id = self.daq_data_id()?;
        let start_index_id = self.start_index_id()?;
        let cal_num_id = get_cal_num(self, video_data_id, daq_data_id, start_index_id);
        let area_id = self.area_id()?;
        let green2_id = decode_all(self, video_data_id, start_index_id, cal_num_id, area_id)?;
        let filter_method_id = self.filter_method_id()?;
        let point_id = PointId::new(self, point);
        filter_point(self, green2_id, filter_method_id, area_id, point_id)
    }

    pub fn get_thermocouples(&self) -> Option<Vec<Thermocouple>> {
        Some(self.thermocouples_id?.thermocouples(self))
    }

    pub fn set_thermocouples(&mut self, thermocouples: Vec<Thermocouple>) {
        self.thermocouples_id = Some(ThermocouplesId::new(self, thermocouples));
    }

    pub fn get_interp_method(&self) -> Option<InterpMethod> {
        Some(self.interp_method_id?.interp_method(self))
    }

    pub fn set_interp_method(&mut self, interp_method: InterpMethod) {
        self.interp_method_id = Some(InterpMethodId::new(self, interp_method));
    }

    pub fn interp_frame(&self, frame_index: usize) -> Result<Array2<f64>, String> {
        let video_data_id = self.video_data_id()?;
        let daq_data_id = self.daq_data_id()?;
        let start_index_id = self.start_index_id()?;
        let cal_num_id = get_cal_num(self, video_data_id, daq_data_id, start_index_id);
        let area_id = self.area_id()?;
        let thermocouples_id = self.thermocouples_id()?;
        let interp_method_id = self.interp_method_id()?;
        let interpolator = make_interpolator(
            self,
            daq_data_id,
            start_index_id,
            cal_num_id,
            area_id,
            thermocouples_id,
            interp_method_id,
        )
        .interpolater(self);
        Ok(interpolator.interp_frame(frame_index))
    }

    pub fn get_iteration_method(&self) -> Option<IterationMethod> {
        Some(self.iteration_method_id?.iteration_method(self))
    }

    pub fn set_iteration_method(&mut self, iteration_method: IterationMethod) {
        self.iteration_method_id = Some(solve::IterationMethodId::new(self, iteration_method));
    }

    pub fn get_physical_param(&self) -> Option<PhysicalParam> {
        Some(self.physical_param_id?.physical_param(self))
    }

    pub fn set_physical_param(&mut self, physical_param: PhysicalParam) {
        self.physical_param_id = Some(solve::PyhsicalParamId::new(self, physical_param));
    }

    fn video_data_id(&self) -> Result<VideoDataId, String> {
        let video_path_id = self.video_path_id.ok_or("video path unset".to_owned())?;
        read_video(self, video_path_id)
    }

    fn daq_data_id(&self) -> Result<DaqDataId, String> {
        let daq_path_id = self.daq_path_id.ok_or("daq path unset".to_owned())?;
        read_daq(self, daq_path_id)
    }

    fn start_index_id(&self) -> Result<StartIndexId, String> {
        self.start_index_id
            .ok_or("video and daq not synchronized yet".to_owned())
    }

    fn area_id(&self) -> Result<AreaId, String> {
        self.area_id.ok_or("area unset".to_owned())
    }

    fn filter_method_id(&self) -> Result<FilterMethodId, String> {
        self.filter_method_id
            .ok_or("filter method unset".to_owned())
    }

    fn thermocouples_id(&self) -> Result<ThermocouplesId, String> {
        self.thermocouples_id.ok_or("thermocouple unset".to_owned())
    }

    fn interp_method_id(&self) -> Result<InterpMethodId, String> {
        self.interp_method_id
            .ok_or("interp method unset".to_owned())
    }
}

#[salsa::interned]
pub(crate) struct StartIndexId {
    pub start_frame: usize,
    pub start_row: usize,
}

#[salsa::tracked]
pub(crate) struct CalNumId {
    pub cal_num: usize,
}

#[salsa::tracked]
pub(crate) fn get_cal_num(
    db: &dyn crate::Db,
    video_data_id: VideoDataId,
    daq_data_id: DaqDataId,
    start_index_id: StartIndexId,
) -> CalNumId {
    let nframes = video_data_id.packets(db).0.len();
    let nrows = daq_data_id.data(db).0.nrows();
    let start_frame = start_index_id.start_frame(db);
    let start_row = start_index_id.start_row(db);
    CalNumId::new(db, (nframes - start_frame).min(nrows - start_row))
}

pub fn init() {
    video::init();
    util::log::init();
}
