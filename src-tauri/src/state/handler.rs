use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use ndarray::{ArcArray2, Array2};
use serde::{Deserialize, Serialize};
use tlc_util::progress_bar::Progress;
use tlc_video::{filter_point, read_video, FilterMethod, VideoId, VideoMeta};
use tracing::{instrument, trace};

use super::GlobalState;
use crate::{
    daq::{read_daq, DaqId, DaqMeta, InterpMethod, Thermocouple},
    post_processing::draw_area,
    setting,
    setting::StartIndex,
    solve::{IterationMethod, NuData, PhysicalParam},
    state::task::TaskId,
};

#[derive(Debug, Deserialize)]
pub struct SettingData {
    pub name: String,
    pub save_root_dir: PathBuf,
    pub video_path: Option<PathBuf>,
    pub daq_path: Option<PathBuf>,
    pub start_frame: Option<usize>,
    pub start_row: Option<usize>,
    pub area: Option<(u32, u32, u32, u32)>,
    pub thermocouples: Option<Vec<Thermocouple>>,
    pub interp_method: Option<InterpMethod>,
    pub filter_method: Option<FilterMethod>,
    pub iteration_method: Option<IterationMethod>,
    pub physical_param: PhysicalParam,
}

impl From<SettingData> for setting::CreateRequest {
    fn from(s: SettingData) -> setting::CreateRequest {
        setting::CreateRequest {
            name: s.name,
            save_root_dir: s.save_root_dir,
            video_path: s.video_path,
            daq_path: s.daq_path,
            start_frame: s.start_frame,
            start_row: s.start_row,
            area: s.area,
            thermocouples: s.thermocouples,
            interp_method: s.interp_method,
            filter_method: s.filter_method.unwrap_or_default(),
            iteration_method: s.iteration_method.unwrap_or_default(),
            physical_param: s.physical_param,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NuView {
    pub nu2: ArcArray2<f64>,
    pub nu_nan_mean: f64,
    pub nu_plot_base64: String,
    pub edge_truncation: (f64, f64),
}

impl GlobalState {
    #[instrument(level = "trace", skip_all, err)]
    pub fn create_setting(&self, setting_data: SettingData) -> Result<()> {
        trace!(?setting_data);
        let mut state = self.inner.lock();
        state.create_setting(setting_data)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn switch_setting(&self, setting_id: i64) -> Result<()> {
        trace!(setting_id);
        let mut state = self.inner.lock();
        state.switch_setting(setting_id)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn delete_setting(&self, setting_id: i64) -> Result<()> {
        trace!(setting_id);
        let mut state = self.inner.lock();
        state.delete_setting(setting_id)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_name(&self) -> Result<String> {
        let state = self.inner.lock();
        state.setting.name(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_name(&self, name: String) -> Result<()> {
        trace!(name);
        let state = self.inner.lock();
        state.setting.set_name(&state.db, &name)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_save_root_dir(&self) -> Result<PathBuf> {
        let state = self.inner.lock();
        state.setting.save_root_dir(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_save_root_dir(&self, save_root_dir: PathBuf) -> Result<()> {
        trace!(?save_root_dir);
        let state = self.inner.lock();
        state.setting.set_save_root_dir(&state.db, &save_root_dir)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_video_path(&self) -> Result<PathBuf> {
        let state = self.inner.lock();
        state.setting.video_path(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_video_path(&self, video_path: PathBuf) -> Result<()> {
        trace!(?video_path);
        let video_id = VideoId { video_path };
        let (task_id, progress_bar) = {
            let mut state = self.inner.lock();
            state.set_video_path(&video_id.video_path)?;
            let task_id = state
                .task_registry
                .register(TaskId::ReadVideo(video_id.clone()))?;
            let progress_bar = state.video_controller.prepare_read_video();
            (task_id, progress_bar)
        };

        let (video_meta, parameters, packet_rx) = read_video(&video_id.video_path, progress_bar)?;
        self.handle_read_video_output1(&video_id, video_meta, parameters)?;
        self.spawn(task_id, move |global_state| {
            let _ =
                global_state.handle_read_video_output2(&video_id, packet_rx, video_meta.nframes);
        });

        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_video_meta(&self) -> Result<VideoMeta> {
        Ok(self.inner.lock().video_data()?.video_meta())
    }

    #[instrument(level = "trace", skip_all, ret)]
    pub fn get_read_video_progress(&self) -> Progress {
        self.inner.lock().video_controller.read_video_progress()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn decode_frame_base64(&self, frame_index: usize) -> Result<String> {
        trace!(frame_index);
        let (decoder_manager, packet) = {
            let state = self.inner.lock();
            let video_data = state.video_data()?;
            let nframes = video_data.video_meta().nframes;
            if frame_index >= nframes {
                bail!("frame_index({frame_index}) >= nframes({nframes})");
            }
            let decoder_manager = video_data.decoder_manager();
            let packet = video_data.packet(frame_index)?;
            (decoder_manager, packet)
        };

        decoder_manager.decode_frame_base64(packet)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_daq_path(&self) -> Result<PathBuf> {
        let state = self.inner.lock();
        state.setting.daq_path(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_daq_path(&self, daq_path: PathBuf) -> Result<()> {
        trace!(?daq_path);
        let _task_id = {
            let mut state = self.inner.lock();
            state.setting.set_daq_path(&state.db, &daq_path)?;
            state.task_registry.register(TaskId::ReadDaq(DaqId {
                daq_path: daq_path.clone(),
            }))?
        };

        let (daq_meta, daq_raw) = read_daq(&daq_path)?;

        self.handle_read_daq_output(&DaqId { daq_path }, daq_meta, daq_raw)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_daq_meta(&self) -> Result<DaqMeta> {
        Ok(self.inner.lock().daq_data()?.daq_meta())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn get_daq_raw(&self) -> Result<ArcArray2<f64>> {
        Ok(self.inner.lock().daq_data()?.daq_raw())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn synchronize_video_and_daq(&self, start_frame: usize, start_row: usize) -> Result<()> {
        trace!(start_frame, start_row);
        let mut state = self.inner.lock();
        state.synchronize_video_and_daq(start_frame, start_row)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_start_index(&self) -> Result<StartIndex> {
        self.inner.lock().start_index()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        trace!(start_frame);
        let mut state = self.inner.lock();
        state.set_start_frame(start_frame)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_start_row(&self, start_row: usize) -> Result<()> {
        trace!(start_row);
        let mut state = self.inner.lock();
        state.set_start_row(start_row)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_area(&self) -> Result<(u32, u32, u32, u32)> {
        self.inner.lock().area()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_area(&self, area: (u32, u32, u32, u32)) -> Result<()> {
        trace!(?area);
        let mut state = self.inner.lock();
        state.set_area(area)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret)]
    pub fn get_build_green2_progress(&self) -> Progress {
        self.inner.lock().video_controller.build_green2_progress()
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_filter_method(&self) -> Result<FilterMethod> {
        let state = self.inner.lock();
        state.setting.filter_method(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        trace!(?filter_method);
        let mut state = self.inner.lock();
        state.set_filter_method(filter_method)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret)]
    pub fn get_detect_peak_progress(&self) -> Progress {
        self.inner.lock().video_controller.detect_peak_progress()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn filter_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
        trace!(?position);
        let (green2, filter_method, area) = {
            let state = self.inner.lock();
            let green2 = state
                .video_data()?
                .green2()
                .ok_or_else(|| anyhow!("green2 not built yet"))?;
            let filter_method = state.setting.filter_method(&state.db)?;
            let area = state.area()?;
            (green2, filter_method, area)
        };

        filter_point(green2, filter_method, area, position)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_thermocouples(&self) -> Result<Vec<Thermocouple>> {
        self.inner.lock().thermocouples()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_thermocouples(&self, thermocouples: Vec<Thermocouple>) -> Result<()> {
        trace!(?thermocouples);
        let mut state = self.inner.lock();
        state.set_thermocouples(&thermocouples)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_interp_method(&self) -> Result<InterpMethod> {
        self.inner.lock().interp_method()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_interp_method(&self, interp_method: InterpMethod) -> Result<()> {
        trace!(?interp_method);
        let mut state = self.inner.lock();
        state.set_interp_method(interp_method)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn interp_frame(&self, frame_index: usize) -> Result<Array2<f64>> {
        trace!(frame_index);
        let interpolator = self.inner.lock().interpolator()?;
        interpolator.interp_frame(frame_index)
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_iteration_method(&self) -> Result<IterationMethod> {
        let state = self.inner.lock();
        state.setting.iteration_method(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        trace!(?iteration_method);
        let mut state = self.inner.lock();
        state.set_iteration_method(iteration_method)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret, err)]
    pub fn get_physical_param(&self) -> Result<PhysicalParam> {
        let state = self.inner.lock();
        state.setting.physical_param(&state.db)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_gmax_temperature(&self, gmax_temperature: f64) -> Result<()> {
        trace!(gmax_temperature);
        let mut state = self.inner.lock();
        state.set_gmax_temperature(gmax_temperature)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_solid_thermal_conductivity(&self, solid_thermal_conductivity: f64) -> Result<()> {
        trace!(solid_thermal_conductivity);
        let mut state = self.inner.lock();
        state.set_solid_thermal_conductivity(solid_thermal_conductivity)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_solid_thermal_diffusivity(&self, solid_thermal_diffusivity: f64) -> Result<()> {
        trace!(solid_thermal_diffusivity);
        let mut state = self.inner.lock();
        state.set_solid_thermal_diffusivity(solid_thermal_diffusivity)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_characteristic_length(&self, characteristic_length: f64) -> Result<()> {
        trace!(characteristic_length);
        let mut state = self.inner.lock();
        state.set_characteristic_length(characteristic_length)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn set_air_thermal_conductivity(&self, air_thermal_conductivity: f64) -> Result<()> {
        trace!(air_thermal_conductivity);
        let mut state = self.inner.lock();
        state.set_air_thermal_conductivity(air_thermal_conductivity)?;
        self.reconcile(state);
        Ok(())
    }

    #[instrument(level = "trace", skip_all, ret)]
    pub fn get_solve_progress(&self) -> Progress {
        self.inner.lock().solve_controller.solve_progress()
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn get_nu(&self, edge_truncation: Option<(f64, f64)>) -> Result<NuView> {
        trace!(?edge_truncation);
        let (nu_data, plot_path) = {
            let state = self.inner.lock();
            let nu_data = state
                .nu_data
                .as_ref()
                .ok_or_else(|| anyhow!("not solved yet"))
                .cloned()?;
            let plot_path = state.plot_path()?;
            (nu_data, plot_path)
        };

        let NuData { nu2, nu_nan_mean } = nu_data;
        let edge_truncation = edge_truncation.unwrap_or((nu_nan_mean * 0.6, nu_nan_mean * 2.0));
        let nu_plot_base64 = draw_area(plot_path, nu2.view(), edge_truncation)?;

        Ok(NuView {
            nu2,
            nu_nan_mean,
            nu_plot_base64,
            edge_truncation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Default for SettingData {
        fn default() -> Self {
            SettingData {
                name: "test_case".to_owned(),
                save_root_dir: PathBuf::from("/tmp"),
                video_path: None,
                daq_path: None,
                start_frame: None,
                start_row: None,
                area: None,
                thermocouples: None,
                interp_method: None,
                filter_method: None,
                iteration_method: None,
                physical_param: PhysicalParam::default(),
            }
        }
    }
}
