#[cfg(test)]
mod tests;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail};
use ndarray::{ArcArray2, Array2};
use serde::Serialize;
use time::{OffsetDateTime, UtcOffset};
use tracing::{info_span, instrument};

use crate::{
    daq::{read_daq, DaqMeta, InterpMethod, Interpolator, Thermocouple},
    postproc::{draw_nu_plot_and_save, nan_mean, save_nu_matrix, save_setting, Setting},
    solve::{solve_nu, IterMethod, PhysicalParam},
    video::{filter_detect_peak, filter_point, read_video, FilterMethod, VideoData, VideoMeta},
};

#[derive(Debug, Serialize)]
pub struct NuData {
    pub nu2: ArcArray2<f64>,
    pub nu_nan_mean: f64,
}

#[instrument(skip_all)]
pub fn reconcile(db_mut: &mut Database, db: &Arc<Mutex<Database>>) {
    if let Some(video_path) = &db_mut.video_path {
        if let Some(tx) = db_mut.video_data.need_execute(video_path) {
            spawn_read_video(db.clone(), video_path.clone(), tx);
        }
    }

    if let Some(daq_path) = &db_mut.daq_path {
        if let Some(tx) = db_mut.daq_data.need_execute(daq_path) {
            spawn_read_daq(db.clone(), daq_path.clone(), tx);
        }
    }
}

fn spawn_read_video(db: Arc<Mutex<Database>>, video_path: PathBuf, tx: async_channel::Sender<()>) {
    std::thread::spawn(move || {
        let _span = info_span!("spawn_read_video").entered();
        _ = tx;

        let video_data = match read_video(&video_path) {
            Ok(video_data) => video_data,
            Err(e) => {
                info_span!("write_db", %e).in_scope(|| {
                    db.lock().unwrap().video_data = TaskState::Failed(e);
                });
                return;
            }
        };

        info_span!("write_db", ?video_path, ?video_data).in_scope(|| {
            db.lock().unwrap().video_data = TaskState::Done {
                input: video_path,
                output: video_data,
            };
        });
    });
}

fn spawn_read_daq(db: Arc<Mutex<Database>>, daq_path: PathBuf, tx: async_channel::Sender<()>) {
    std::thread::spawn(move || {
        let _span = info_span!("spawn_daq_video").entered();
        _ = tx;

        let daq_data = match read_daq(&daq_path) {
            Ok(daq_data) => daq_data,
            Err(e) => {
                info_span!("write_db", %e).in_scope(|| {
                    db.lock().unwrap().daq_data = TaskState::Failed(e);
                });
                return;
            }
        };

        info_span!(
            "write_db",
            ?daq_path,
            nrows = daq_data.nrows(),
            ncols = daq_data.ncols()
        )
        .in_scope(|| {
            db.lock().unwrap().daq_data = TaskState::Done {
                input: daq_path,
                output: daq_data,
            };
        });
    });
}

#[derive(Default)]
pub struct Database {
    /// User defined unique name of this experiment setting.
    pub name: String,
    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/{expertiment_name}_setting.json
    /// * nu_matrix_path: {root_dir}/{expertiment_name}_nu_matrix.csv
    /// * nu_plot_path: {root_dir}/{expertiment_name}_nu_plot.png
    save_root_dir: Option<PathBuf>,
    video_path: Option<PathBuf>,
    daq_path: Option<PathBuf>,
    /// Start frame of video and start row of DAQ data involved in the calculation,
    /// updated simultaneously.
    start_index: Option<StartIndex>,
    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    area: Option<(u32, u32, u32, u32)>,
    /// Columns in the csv file and positions of thermocouples.
    thermocouples: Option<Box<[Thermocouple]>>,
    /// Filter method of green matrix along the time axis.
    filter_method: Option<FilterMethod>,
    /// Interpolation method for calculating thermocouple temperature distribution.
    interp_method: Option<InterpMethod>,
    /// Iteration method for solving heat transfer equataion.
    iter_method: Option<IterMethod>,
    /// All physical parameters used when solving heat transfer equation.
    physical_param: Option<PhysicalParam>,

    video_data: TaskState<PathBuf, Arc<VideoData>>,
    daq_data: TaskState<PathBuf, ArcArray2<f64>>,
}

#[derive(Debug, Default)]
pub enum TaskState<I: PartialEq + Clone, O> {
    #[default]
    NotStarted,
    InProcess {
        input: I,
        waker: async_channel::Receiver<()>,
    },
    Failed(anyhow::Error),
    Done {
        input: I,
        output: O,
    },
}

impl<I: PartialEq + Clone, O> TaskState<I, O> {
    fn need_execute(&mut self, new_input: &I) -> Option<async_channel::Sender<()>> {
        match self {
            TaskState::Done { input, .. } if input == new_input => return None,
            TaskState::InProcess { input, .. } if input == new_input => return None,
            TaskState::InProcess { waker, .. } => _ = waker.close(),
            _ => {}
        }

        let (tx, rx) = async_channel::bounded(1);
        *self = TaskState::InProcess {
            input: new_input.clone(),
            waker: rx,
        };

        Some(tx)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StartIndex {
    pub start_frame: usize,
    pub start_row: usize,
}

// Operation setting an input of some heavy computations will cause blocking.
impl Database {
    fn save_root_dir(&self) -> anyhow::Result<&Path> {
        Ok(self
            .save_root_dir
            .as_ref()
            .ok_or_else(|| anyhow!("save root dir unset"))?)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_save_root_dir(&mut self, save_root_dir: PathBuf) -> anyhow::Result<()> {
        if !save_root_dir.exists() {
            bail!("{save_root_dir:?} not exists");
        }
        self.save_root_dir = Some(save_root_dir);
        Ok(())
    }

    fn video_path(&self) -> anyhow::Result<&Path> {
        Ok(self
            .video_path
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?)
    }

    fn set_video_path(&mut self, video_path: PathBuf) -> anyhow::Result<()> {
        if !video_path.exists() {
            bail!("{video_path:?} not exists");
        }
        self.start_index = None;
        self.video_path = Some(video_path);
        Ok(())
    }

    fn video_nframes(&self) -> anyhow::Result<usize> {
        Ok(self.video_data()?.nframes())
    }

    fn video_frame_rate(&self) -> anyhow::Result<usize> {
        Ok(self.video_data()?.frame_rate())
    }

    fn video_shape(&self) -> anyhow::Result<(u32, u32)> {
        Ok(self.video_data()?.shape())
    }

    fn daq_path(&self) -> anyhow::Result<&Path> {
        Ok(self
            .daq_path
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?)
    }

    fn set_daq_path(&mut self, daq_path: PathBuf) -> anyhow::Result<()> {
        if !daq_path.exists() {
            bail!("{daq_path:?} not exists");
        }
        self.start_index = None;
        self.daq_path = Some(daq_path);
        Ok(())
    }

    fn daq_data(&self) -> anyhow::Result<ArcArray2<f64>> {
        read_daq(self.daq_path()?)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
    ) -> anyhow::Result<()> {
        let nframes = self.video_nframes()?;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.nrows();
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }
        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });
        Ok(())
    }

    fn start_frame(&self) -> anyhow::Result<usize> {
        Ok(self.start_index()?.start_frame)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_start_frame(&mut self, start_frame: usize) -> anyhow::Result<()> {
        let nframes = self.video_nframes()?;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.nrows();
        let old_start_frame = self.start_frame()?;
        let old_start_row = self.start_row()?;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }
        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });
        Ok(())
    }

    fn start_row(&self) -> anyhow::Result<usize> {
        Ok(self.start_index()?.start_row)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_start_row(&mut self, start_row: usize) -> anyhow::Result<()> {
        let nrows = self.daq_data()?.nrows();
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }
        let nframes = self.video_nframes()?;
        let old_start_frame = self.start_frame()?;
        let old_start_row = self.start_row()?;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_frame");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }
        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });
        Ok(())
    }

    fn area(&self) -> anyhow::Result<(u32, u32, u32, u32)> {
        self.area.ok_or_else(|| anyhow!("area unset"))
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_area(&mut self, area: (u32, u32, u32, u32)) -> anyhow::Result<()> {
        let (h, w) = self.video_shape()?;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})");
        }
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})");
        }
        self.area = Some(area);
        Ok(())
    }

    fn filter_method(&self) -> anyhow::Result<FilterMethod> {
        self.filter_method
            .ok_or_else(|| anyhow!("filter method unset"))
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_filter_method(&mut self, filter_method: FilterMethod) -> anyhow::Result<()> {
        match filter_method {
            FilterMethod::No => {}
            FilterMethod::Median { window_size } => {
                if window_size == 0 {
                    bail!("window size can not be zero");
                }
                if window_size > self.video_nframes()? / 10 {
                    bail!("window size too large: {window_size}");
                }
            }
            FilterMethod::Wavelet { threshold_ratio } => {
                if !(0.0..1.0).contains(&threshold_ratio) {
                    bail!("thershold ratio must belong to (0, 1): {threshold_ratio}");
                }
            }
        }
        self.filter_method = Some(filter_method);
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    fn filter_point(&self, point: (usize, usize)) -> anyhow::Result<Vec<u8>> {
        let video_data = self.video_data()?;
        let daq_data = self.daq_data()?;
        let start_index = self.start_index()?;
        let cal_num = eval_cal_num(video_data.nframes(), daq_data.nrows(), start_index);
        let area = self.area()?;
        let green2 = video_data.decode_all(start_index.start_frame, cal_num, area)?;
        let filter_method = self.filter_method()?;
        filter_point(green2, filter_method, area, point)
    }

    fn thermocouples(&self) -> anyhow::Result<&[Thermocouple]> {
        Ok(self
            .thermocouples
            .as_ref()
            .ok_or_else(|| anyhow!("thermocouples unset"))?)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_thermocouples(&mut self, thermocouples: Box<[Thermocouple]>) -> anyhow::Result<()> {
        let daq_ncols = self.daq_data()?.ncols();
        for thermocouple in &*thermocouples {
            let column_index = thermocouple.column_index;
            if thermocouple.column_index >= daq_ncols {
                bail!("thermocouple column_index({column_index}) exceeds daq ncols({daq_ncols})");
            }
        }
        self.thermocouples = Some(thermocouples);
        Ok(())
    }

    fn interp_method(&self) -> anyhow::Result<InterpMethod> {
        self.interp_method
            .ok_or_else(|| anyhow!("interp method unset"))
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_interp_method(&mut self, interp_method: InterpMethod) -> anyhow::Result<()> {
        let thermocouples = self.thermocouples()?;
        if let InterpMethod::Bilinear(y, x) | InterpMethod::BilinearExtra(y, x) = interp_method {
            if (x * y) as usize != thermocouples.len() {
                bail!("invalid interp method");
            }
        }
        self.interp_method = Some(interp_method);
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    fn interp_frame(&self, frame_index: usize) -> anyhow::Result<Array2<f64>> {
        let video_data = self.video_data()?;
        let daq_data = self.daq_data()?;
        let start_index = self.start_index()?;
        let cal_num = eval_cal_num(video_data.nframes(), daq_data.nrows(), start_index);
        let area = self.area()?;
        let thermocouples = self.thermocouples()?;
        let interp_method = self.interp_method()?;
        let interpolator = Interpolator::new(
            start_index.start_row,
            cal_num,
            area,
            interp_method,
            thermocouples,
            daq_data.view(),
        );
        Ok(interpolator.interp_frame(frame_index))
    }

    fn iter_method(&self) -> anyhow::Result<IterMethod> {
        self.iter_method.ok_or_else(|| anyhow!("iter method unset"))
    }

    #[instrument(level = "trace", skip(self), err)]
    fn set_iter_method(&mut self, iter_method: IterMethod) -> anyhow::Result<()> {
        match iter_method {
            IterMethod::NewtonTangent { h0, .. } | IterMethod::NewtonDown { h0, .. }
                if h0.is_nan() =>
            {
                bail!("invalid iter method: {iter_method:?}");
            }
            _ => {}
        }
        self.iter_method = Some(iter_method);
        Ok(())
    }

    fn physical_param(&self) -> anyhow::Result<PhysicalParam> {
        self.physical_param
            .ok_or_else(|| anyhow!("physical param unset"))
    }

    fn set_physical_param(&mut self, physical_param: PhysicalParam) -> anyhow::Result<()> {
        if physical_param.gmax_temperature.is_nan()
            || physical_param.characteristic_length.is_nan()
            || physical_param.air_thermal_conductivity.is_nan()
            || physical_param.solid_thermal_diffusivity.is_nan()
            || physical_param.solid_thermal_conductivity.is_nan()
        {
            bail!("invalid physical param: {physical_param:?}");
        }
        self.physical_param = Some(physical_param);
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err)]
    fn nu_plot(&self, trunc: Option<(f64, f64)>) -> anyhow::Result<String> {
        let name = &self.name;
        let save_root_dir = self.save_root_dir()?;
        let nu2 = self.nu2()?;
        let nu_plot_path = save_root_dir.join(format!("{name}_nu_plot.png"));
        draw_nu_plot_and_save(nu2.view(), trunc, nu_plot_path)
    }

    fn video_data(&self) -> anyhow::Result<Arc<VideoData>> {
        read_video(self.video_path()?)
    }

    #[instrument(level = "trace", skip(self), err)]
    fn save_data(&self) -> anyhow::Result<()> {
        let name = &self.name;
        let save_root_dir = self.save_root_dir()?;

        let nu2 = self.nu2()?;
        let nu_matrix_path = save_root_dir.join(format!("{name}_nu_matrix.csv"));
        save_nu_matrix(nu2.view(), nu_matrix_path)?;

        let video_data = self.video_data()?;
        let video_meta = VideoMeta {
            frame_rate: video_data.frame_rate(),
            nframes: video_data.nframes(),
            shape: video_data.shape(),
        };
        let daq_data = self.daq_data()?;
        let daq_meta = DaqMeta {
            nrows: daq_data.nrows(),
            ncols: daq_data.ncols(),
        };
        let setting = Setting {
            name,
            save_root_dir,
            video_path: self.video_path()?,
            video_meta,
            daq_path: self.daq_path()?,
            daq_meta,
            start_frame: self.start_frame()?,
            start_row: self.start_row()?,
            area: self.area()?,
            thermocouples: self.thermocouples()?,
            filter_method: self.filter_method()?,
            interp_method: self.interp_method()?,
            iter_method: self.iter_method()?,
            physical_param: self.physical_param()?,
            saved_at: OffsetDateTime::now_utc().to_offset(UtcOffset::from_hms(8, 0, 0).unwrap()),
            nu_nan_mean: nan_mean(nu2.view()),
        };
        let setting_path = save_root_dir.join(format!("{name}_setting.json"));
        save_setting(setting, setting_path)?;

        Ok(())
    }

    fn nu2(&self) -> anyhow::Result<ArcArray2<f64>> {
        let video_data = self.video_data()?;
        let daq_data = self.daq_data()?;
        let start_index = self.start_index()?;
        let cal_num = eval_cal_num(video_data.nframes(), daq_data.nrows(), start_index);
        let area = self.area()?;
        let green2 = video_data.decode_all(start_index.start_frame, cal_num, area)?;
        let filter_method = self.filter_method()?;
        let gmax_frame_indexes = filter_detect_peak(green2, filter_method);
        let thermocouples = self.thermocouples()?;
        let interp_method = self.interp_method()?;
        let interpolator = Interpolator::new(
            start_index.start_row,
            cal_num,
            area,
            interp_method,
            thermocouples,
            daq_data.view(),
        );
        let physical_param = self.physical_param()?;
        let iteration_method = self.iter_method()?;
        let nu2 = solve_nu(
            video_data.frame_rate(),
            &gmax_frame_indexes,
            interpolator,
            physical_param,
            iteration_method,
        )
        .into_shared();
        Ok(nu2)
    }

    fn start_index(&self) -> anyhow::Result<StartIndex> {
        self.start_index
            .ok_or_else(|| anyhow!("video and daq not synchronized yet"))
    }
}

pub fn eval_cal_num(nframes: usize, nrows: usize, start_index: StartIndex) -> usize {
    let start_frame = start_index.start_frame;
    let start_row = start_index.start_row;
    (nframes - start_frame).min(nrows - start_row)
}
