use std::path::{Path, PathBuf};

use ndarray::{ArcArray2, Array2};
use salsa::{DebugWithDb, Snapshot};
use serde::Serialize;
use tracing::{instrument, trace};

use crate::{
    daq::{
        make_interpolator, read_daq, DaqDataId, DaqMeta, DaqPathId, InterpMethod, InterpMethodId,
        Thermocouple, ThermocouplesId,
    },
    post_processing::{draw_nu_plot_and_save, nan_mean, save_nu_matrix, save_setting},
    solve::{solve_nu, IterMethod, IterMethodId, Nu2Id, PhysicalParam, PhysicalParamId},
    video::{
        self, decode_all, filter_detect_peak, filter_point, read_video, AreaId, FilterMethod,
        FilterMethodId, PointId, VideoDataId, VideoMeta, VideoPathId,
    },
    Jar,
};

#[derive(Default)]
#[salsa::db(Jar)]
pub struct Database {
    storage: salsa::Storage<Self>,
    /// User defined unique name of this experiment setting.
    name: Option<String>,
    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/{expertiment_name}_setting.json
    /// * nu_matrix_path: {root_dir}/{expertiment_name}_nu_matrix.csv
    /// * nu_plot_path: {root_dir}/{expertiment_name}_nu_plot.png
    save_root_dir: Option<PathBuf>,
    video_path_id: Option<VideoPathId>,
    daq_path_id: Option<DaqPathId>,
    /// Start frame of video and start row of DAQ data involved in the calculation,
    /// updated simultaneously.
    start_index_id: Option<StartIndexId>,
    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    area_id: Option<AreaId>,
    /// Columns in the csv file and positions of thermocouples.
    thermocouples_id: Option<ThermocouplesId>,
    /// Filter method of green matrix along the time axis.
    filter_method_id: Option<FilterMethodId>,
    /// Interpolation method for calculating thermocouple temperature distribution.
    interp_method_id: Option<InterpMethodId>,
    /// Iteration method for solving heat transfer equataion.
    iter_method_id: Option<IterMethodId>,
    /// All physical parameters used when solving heat transfer equation.
    physical_param_id: Option<PhysicalParamId>,
}

impl salsa::Database for Database {
    fn salsa_event(&self, event: salsa::Event) {
        match event.kind {
            salsa::EventKind::WillCheckCancellation => {}
            _ => trace!("salsa_event: {:?}", event.debug(self)),
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
            iter_method_id: self.iter_method_id,
        })
    }
}

impl crate::Db for Database {}

/// `Setting` will be saved together with the results for later check.
#[derive(Debug, Serialize)]
pub(crate) struct Setting<'a> {
    pub name: &'a str,
    pub save_root_dir: &'a Path,
    pub video_path: &'a Path,
    pub video_meta: VideoMeta,
    pub daq_path: &'a Path,
    pub daq_meta: DaqMeta,
    pub start_frame: usize,
    pub start_row: usize,
    pub area: (u32, u32, u32, u32),
    pub thermocouples: &'a [Thermocouple],
    pub filter_method: FilterMethod,
    pub interp_method: InterpMethod,
    pub iter_method: IterMethod,
    pub physical_param: PhysicalParam,
    /// Final result.
    pub nu_nan_mean: f64,
    /// Timestamp in milliseconds.
    #[serde(with = "time::serde::rfc3339")]
    pub saved_at: time::OffsetDateTime,
}

// Operation setting an input of some heavy computations will cause blocking.
impl Database {
    pub fn get_name(&self) -> Result<&str, String> {
        Ok(self.name.as_ref().ok_or("name unset")?)
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_name(&mut self, name: String) -> Result<(), String> {
        if name.is_empty() {
            return Err("empty name".to_owned());
        }
        self.name = Some(name);
        Ok(())
    }

    pub fn get_save_root_dir(&self) -> Result<&Path, String> {
        Ok(self.save_root_dir.as_ref().ok_or("save root dir unset")?)
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_save_root_dir(&mut self, save_root_dir: PathBuf) -> Result<(), String> {
        if !save_root_dir.exists() {
            return Err(format!("{save_root_dir:?} not exists"));
        }
        self.save_root_dir = Some(save_root_dir);
        Ok(())
    }

    pub fn get_video_path(&self) -> Result<&Path, String> {
        Ok(self.video_path_id.ok_or("video path unset")?.path(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_video_path(&mut self, video_path: PathBuf) -> Result<(), String> {
        if !video_path.exists() {
            return Err(format!("{video_path:?} not exists"));
        }
        self.start_index_id = None;
        match self.video_path_id {
            Some(video_path_id) => {
                video_path_id.set_path(self).to(video_path);
            }
            None => self.video_path_id = Some(VideoPathId::new(self, video_path)),
        }
        Ok(())
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

    pub fn get_daq_path(&self) -> Result<&Path, String> {
        Ok(self.daq_path_id.ok_or("daq path unset")?.path(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_daq_path(&mut self, daq_path: PathBuf) -> Result<(), String> {
        if !daq_path.exists() {
            return Err(format!("{daq_path:?} not exists"));
        }
        self.start_index_id = None;
        match self.daq_path_id {
            Some(daq_path_id) => {
                daq_path_id.set_path(self).to(daq_path);
            }
            None => self.daq_path_id = Some(DaqPathId::new(self, daq_path)),
        }
        Ok(())
    }

    pub fn get_daq_data(&self) -> Result<ArcArray2<f64>, String> {
        Ok(read_daq(self, self.daq_path_id()?)?.data(self).0)
    }

    #[instrument(level = "trace", skip(self), err)]
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

    #[instrument(level = "trace", skip(self), err)]
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

    #[instrument(level = "trace", skip(self), err)]
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

    pub fn get_area(&self) -> Result<(u32, u32, u32, u32), String> {
        Ok(self.area_id.ok_or("area unset")?.area(self))
    }

    #[instrument(level = "trace", skip(self), err)]
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

    pub fn get_filter_method(&self) -> Result<FilterMethod, String> {
        Ok(self
            .filter_method_id
            .ok_or("filter method unset")?
            .filter_method(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_filter_method(&mut self, filter_method: FilterMethod) -> Result<(), String> {
        match filter_method {
            FilterMethod::No => {}
            FilterMethod::Median { window_size } => {
                if window_size == 0 {
                    return Err("window size can not be zero".to_owned());
                }
                if window_size > self.get_video_nframes()? / 10 {
                    return Err(format!("window size too large: {window_size}"));
                }
            }
            FilterMethod::Wavelet { threshold_ratio } => {
                if !(0.0..1.0).contains(&threshold_ratio) {
                    return Err(format!(
                        "thershold ratio must belong to (0, 1): {threshold_ratio}"
                    ));
                }
            }
        }
        self.filter_method_id = Some(FilterMethodId::new(self, filter_method));
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn filter_point(&self, point: (usize, usize)) -> Result<Vec<u8>, String> {
        let video_data_id = self.video_data_id()?;
        let daq_data_id = self.daq_data_id()?;
        let start_index_id = self.start_index_id()?;
        let cal_num_id = eval_cal_num(self, video_data_id, daq_data_id, start_index_id);
        let area_id = self.area_id()?;
        let green2_id = decode_all(self, video_data_id, start_index_id, cal_num_id, area_id)?;
        let filter_method_id = self.filter_method_id()?;
        let point_id = PointId::new(self, point);
        filter_point(self, green2_id, filter_method_id, area_id, point_id)
    }

    pub fn get_thermocouples(&self) -> Result<&[Thermocouple], String> {
        Ok(self
            .thermocouples_id
            .ok_or("thermocouples unset")?
            .thermocouples(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_thermocouples(&mut self, thermocouples: Vec<Thermocouple>) -> Result<(), String> {
        let daq_ncols = self.get_daq_data()?.ncols();
        for thermocouple in &thermocouples {
            let column_index = thermocouple.column_index;
            if thermocouple.column_index >= daq_ncols {
                return Err(format!(
                    "thermocouple column_index({column_index}) exceeds daq ncols({daq_ncols})"
                ));
            }
        }
        self.thermocouples_id = Some(ThermocouplesId::new(self, thermocouples));
        Ok(())
    }

    pub fn get_interp_method(&self) -> Result<InterpMethod, String> {
        Ok(self
            .interp_method_id
            .ok_or("interp method unset")?
            .interp_method(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_interp_method(&mut self, interp_method: InterpMethod) -> Result<(), String> {
        let thermocouples = self.get_thermocouples()?;
        if let InterpMethod::Bilinear(y, x) | InterpMethod::BilinearExtra(y, x) = interp_method {
            if (x * y) as usize != thermocouples.len() {
                return Err("invalid interp method".to_owned());
            }
        }
        self.interp_method_id = Some(InterpMethodId::new(self, interp_method));
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn interp_frame(&self, frame_index: usize) -> Result<Array2<f64>, String> {
        let video_data_id = self.video_data_id()?;
        let daq_data_id = self.daq_data_id()?;
        let start_index_id = self.start_index_id()?;
        let cal_num_id = eval_cal_num(self, video_data_id, daq_data_id, start_index_id);
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

    pub fn get_iter_method(&self) -> Result<IterMethod, String> {
        Ok(self
            .iter_method_id
            .ok_or("iter method unset")?
            .iter_method(self))
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn set_iter_method(&mut self, iter_method: IterMethod) -> Result<(), String> {
        match iter_method {
            IterMethod::NewtonTangent { h0, .. } | IterMethod::NewtonDown { h0, .. }
                if h0.is_nan() =>
            {
                return Err("invalid iter method: {iter_method:?}".to_owned())
            }
            _ => {}
        }
        self.iter_method_id = Some(IterMethodId::new(self, iter_method));
        Ok(())
    }

    pub fn get_physical_param(&self) -> Result<PhysicalParam, String> {
        Ok(self
            .physical_param_id
            .ok_or("physical param unset")?
            .physical_param(self))
    }

    pub fn set_physical_param(&mut self, physical_param: PhysicalParam) -> Result<(), String> {
        if physical_param.gmax_temperature.is_nan()
            || physical_param.characteristic_length.is_nan()
            || physical_param.air_thermal_conductivity.is_nan()
            || physical_param.solid_thermal_diffusivity.is_nan()
            || physical_param.solid_thermal_conductivity.is_nan()
        {
            return Err(format!("invalid physical param: {physical_param:?}"));
        }
        self.physical_param_id = Some(PhysicalParamId::new(self, physical_param));
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn get_nu_data(&self) -> Result<NuData, String> {
        let nu2 = self.nu2_id()?.nu2(self).0;
        let nu_nan_mean = nan_mean(nu2.view());
        Ok(NuData { nu2, nu_nan_mean })
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn get_nu_plot(&self, trunc: Option<(f64, f64)>) -> Result<String, String> {
        let name = self.get_name()?;
        let save_root_dir = self.get_save_root_dir()?;
        let nu2 = self.nu2_id()?.nu2(self).0;
        let nu_plot_path = save_root_dir.join(format!("{name}_nu_plot.png"));
        draw_nu_plot_and_save(nu2.view(), trunc, nu_plot_path)
    }

    #[instrument(level = "trace", skip(self), err)]
    pub fn save_data(&self) -> Result<(), String> {
        let name = self.get_name()?;
        let save_root_dir = self.get_save_root_dir()?;

        let nu2 = self.nu2_id()?.nu2(self).0;
        let nu_matrix_path = save_root_dir.join(format!("{name}_nu_matrix.csv"));
        save_nu_matrix(nu2.view(), nu_matrix_path).map_err(|e| e.to_string())?;

        let video_data_id = self.video_data_id()?;
        let video_meta = VideoMeta {
            frame_rate: video_data_id.frame_rate(self),
            nframes: video_data_id.packets(self).0.len(),
            shape: video_data_id.shape(self),
        };
        let daq_data_id = self.daq_data_id()?;
        let daq_meta = DaqMeta {
            nrows: daq_data_id.data(self).0.nrows(),
            ncols: daq_data_id.data(self).0.nrows(),
        };
        let setting = Setting {
            name,
            save_root_dir,
            video_path: self.get_video_path()?,
            video_meta,
            daq_path: self.get_daq_path()?,
            daq_meta,
            start_frame: self.get_start_frame()?,
            start_row: self.get_start_row()?,
            area: self.get_area()?,
            thermocouples: self.get_thermocouples()?,
            filter_method: self.get_filter_method()?,
            interp_method: self.get_interp_method()?,
            iter_method: self.get_iter_method()?,
            physical_param: self.get_physical_param()?,
            saved_at: time::OffsetDateTime::now_local().unwrap(),
            nu_nan_mean: nan_mean(nu2.view()),
        };
        let setting_path = save_root_dir.join("{name}_setting.json");
        save_setting(setting, setting_path)?;

        Ok(())
    }

    fn video_path_id(&self) -> Result<VideoPathId, String> {
        self.video_path_id.ok_or("video path unset".to_owned())
    }

    fn daq_path_id(&self) -> Result<DaqPathId, String> {
        self.daq_path_id.ok_or("daq path unset".to_owned())
    }

    fn video_data_id(&self) -> Result<VideoDataId, String> {
        read_video(self, self.video_path_id()?)
    }

    fn daq_data_id(&self) -> Result<DaqDataId, String> {
        read_daq(self, self.daq_path_id()?)
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

    fn physical_param_id(&self) -> Result<PhysicalParamId, String> {
        self.physical_param_id
            .ok_or("physical param unset".to_owned())
    }

    fn iter_method_id(&self) -> Result<IterMethodId, String> {
        self.iter_method_id.ok_or("iter method unset".to_owned())
    }

    fn nu2_id(&self) -> Result<Nu2Id, String> {
        let video_data_id = self.video_data_id()?;
        let daq_data_id = self.daq_data_id()?;
        let start_index_id = self.start_index_id()?;
        let cal_num_id = eval_cal_num(self, video_data_id, daq_data_id, start_index_id);
        let area_id = self.area_id()?;
        let green2_id = decode_all(self, video_data_id, start_index_id, cal_num_id, area_id)?;
        let filter_method_id = self.filter_method_id()?;
        let gmax_frame_indexes_id = filter_detect_peak(self, green2_id, filter_method_id);
        let thermocouples_id = self.thermocouples_id()?;
        let interp_method_id = self.interp_method_id()?;
        let interpolator_id = make_interpolator(
            self,
            daq_data_id,
            start_index_id,
            cal_num_id,
            area_id,
            thermocouples_id,
            interp_method_id,
        );
        let physical_param_id = self.physical_param_id()?;
        let iteration_method_id = self.iter_method_id()?;
        let nu2_id = solve_nu(
            self,
            video_data_id,
            gmax_frame_indexes_id,
            interpolator_id,
            physical_param_id,
            iteration_method_id,
        );
        Ok(nu2_id)
    }
}

pub async fn decode_frame_base64(
    db: Snapshot<Database>,
    frame_index: usize,
) -> Result<String, String> {
    let video_data_id = db.video_data_id()?;
    let decoder_manager = video_data_id.decoder_manager(&*db);
    let packets = video_data_id.packets(&*db).0;
    video::decode_frame_base64(decoder_manager, packets, frame_index)
        .await
        .map_err(|e| e.to_string())
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
pub(crate) fn eval_cal_num(
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

#[derive(Debug, Serialize)]
pub struct NuData {
    pub nu2: ArcArray2<f64>,
    pub nu_nan_mean: f64,
}
