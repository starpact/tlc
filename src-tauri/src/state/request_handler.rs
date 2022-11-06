use std::{
    path::{Path, PathBuf},
    thread::spawn,
};

use anyhow::{anyhow, bail, Result};
use ndarray::{ArcArray2, Array2};
use tlc_util::progress_bar::Progress;
use tlc_video::{filter_point, FilterMethod, VideoId, VideoMeta};
use tracing::{instrument, trace, warn};

use super::GlobalState;
use crate::{
    daq::{DaqId, DaqMeta, InterpMethod, Thermocouple},
    post_processing::draw_area,
    request::{NuView, Responder, SettingData},
    setting::StartIndex,
    solve::{IterationMethod, NuData, PhysicalParam},
};

impl GlobalState {
    #[instrument(level = "trace", skip_all)]
    pub fn on_create_setting(
        &mut self,
        create_setting: Box<SettingData>,
        responder: Responder<()>,
    ) {
        trace!(?create_setting);
        responder.respond(
            self.setting
                .create_setting(&self.db, (*create_setting).into()),
        );
        self.reconcile();
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_switch_setting(&mut self, setting_id: i64, responder: Responder<()>) {
        trace!(setting_id);
        responder.respond(self.setting.switch_setting(&self.db, setting_id));
        self.reconcile();
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_delete_setting(&mut self, setting_id: i64, responder: Responder<()>) {
        trace!(setting_id);
        responder.respond(self.setting.delete_setting(&self.db, setting_id));
        self.reconcile();
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_name(&self, responder: Responder<String>) {
        responder.respond(self.setting.name(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_name(&self, name: String, responder: Responder<()>) {
        trace!(name);
        responder.respond(self.setting.set_name(&self.db, &name));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_save_root_dir(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting.save_root_dir(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_save_root_dir(&mut self, save_root_dir: PathBuf, responder: Responder<()>) {
        trace!(?save_root_dir);
        responder.respond(self.setting.set_save_root_dir(&self.db, &save_root_dir));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_video_path(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting.video_path(&self.db))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_video_path(&mut self, video_path: PathBuf, responder: Responder<()>) {
        trace!(?video_path);
        if let Err(e) = self.set_video_path(&video_path) {
            responder.respond_err(e);
            return;
        }

        self.spawn_read_video(VideoId { video_path }, Some(responder));
    }

    fn set_video_path(&mut self, video_path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_video_path(&tx, video_path)?;
        self.setting.set_area(&tx, None)?;
        self.setting.set_start_index(&tx, None)?;
        self.setting.set_thermocouples(&tx, None)?;
        tx.commit()?;

        self.video_data = None;
        if let Some(daq_data) = self.daq_data.as_mut() {
            daq_data.set_interpolator(None);
        }
        self.nu_data = None;

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_video_meta(&self, responder: Responder<VideoMeta>) {
        responder.respond(self.video_data().map(|video_data| video_data.video_meta()))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_read_video_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.read_video_progress());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_decode_frame_base64(&self, frame_index: usize, responder: Responder<String>) {
        trace!(?frame_index);
        let f = || {
            let video_data = self.video_data()?;
            let nframes = video_data.video_meta().nframes;
            if frame_index >= nframes {
                bail!("frame_index({frame_index}) exceeds nframes({nframes})");
            }
            let packet = video_data.packet(frame_index)?;
            Ok((video_data, packet))
        };

        let (video_data, packet) = match f() {
            Ok(ret) => ret,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };
        let decoder_manager = video_data.decoder_manager();

        spawn(move || responder.respond_no_result_log(decoder_manager.decode_frame_base64(packet)));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_daq_path(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting.daq_path(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_daq_path(&mut self, daq_path: PathBuf, responder: Responder<()>) {
        trace!(?daq_path);
        if let Err(e) = self.set_daq_path(&daq_path) {
            responder.respond_err(e);
            return;
        }

        self.spawn_read_daq(DaqId { daq_path }, Some(responder));
    }

    fn set_daq_path(&mut self, daq_path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_daq_path(&tx, daq_path)?;
        self.setting.set_start_index(&tx, None)?;
        self.setting.set_thermocouples(&tx, None)?;
        tx.commit()?;

        self.daq_data = None;
        if let Some(video_data) = self.video_data.as_mut() {
            video_data.set_green2(None).set_gmax_frame_indexes(None);
        }
        self.nu_data = None;

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_daq_meta(&self, responder: Responder<DaqMeta>) {
        responder.respond(self.daq_data().map(|daq_data| daq_data.daq_meta()));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_daq_raw(&self, responder: Responder<ArcArray2<f64>>) {
        responder.respond_no_result_log(self.daq_data().map(|daq_data| daq_data.daq_raw()))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_start_index(&self, responder: Responder<StartIndex>) {
        responder.respond(self.start_index());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
        responder: Responder<()>,
    ) {
        trace!(start_frame, start_row);
        responder.respond(self.synchronize_video_and_daq(start_frame, start_row));
    }

    fn synchronize_video_and_daq(&mut self, start_frame: usize, start_row: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_start_frame(&mut self, start_frame: usize, responder: Responder<()>) {
        trace!(start_frame);
        responder.respond(self.set_start_frame(start_frame));
    }

    fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.daq_meta().nrows;
        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_start_row(&mut self, start_row: usize, responder: Responder<()>) {
        trace!(start_row);
        responder.respond(self.set_start_row(start_row));
    }

    fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }
        let nframes = self.video_data()?.video_meta().nframes;
        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_area(&self, responder: Responder<(u32, u32, u32, u32)>) {
        responder.respond(self.area());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_area(&mut self, area: (u32, u32, u32, u32), responder: Responder<()>) {
        trace!(?area);
        responder.respond(self.set_area(area));
    }

    fn set_area(&mut self, area: (u32, u32, u32, u32)) -> Result<()> {
        let (h, w) = self.video_data()?.video_meta().shape;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})");
        }
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})");
        }

        self.setting.set_area(&self.db, Some(area))?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        if let Some(daq_data) = self.daq_data.as_mut() {
            daq_data.set_interpolator(None);
        }
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_build_green2_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.build_green2_progress());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_filter_method(&self, responder: Responder<FilterMethod>) {
        responder.respond(self.setting.filter_method(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_filter_method(&mut self, filter_method: FilterMethod, responder: Responder<()>) {
        trace!(?filter_method);
        responder.respond(self.set_filter_method(filter_method));
    }

    fn set_filter_method(&mut self, filter_method: FilterMethod) -> Result<()> {
        self.setting.set_filter_method(&self.db, filter_method)?;
        if let Some(video_data) = self.video_data.as_mut() {
            video_data.set_gmax_frame_indexes(None);
        }
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_detect_peak_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.detect_peak_progress());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_filter_point(&self, position: (usize, usize), responder: Responder<Vec<u8>>) {
        trace!(?position);
        let f = || {
            let green2 = self
                .video_data()?
                .green2()
                .ok_or_else(|| anyhow!("green2 not built yet"))?;
            let filter_method = self.setting.filter_method(&self.db)?;
            let area = self.area()?;

            Ok((green2, filter_method, area))
        };

        let (green2, filter_method, area) = match f() {
            Ok(ret) => ret,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };

        spawn(move || responder.respond(filter_point(green2, filter_method, area, position)));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_thermocouples(&self, responder: Responder<Vec<Thermocouple>>) {
        responder.respond(self.thermocouples());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_thermocouples(
        &mut self,
        thermocouples: Vec<Thermocouple>,
        responder: Responder<()>,
    ) {
        trace!(?thermocouples);
        responder.respond(self.set_thermocouples(&thermocouples));
    }

    fn set_thermocouples(&mut self, thermocouples: &[Thermocouple]) -> Result<()> {
        if thermocouples.len() == 1 {
            bail!("there must be at least two thermocouples");
        }

        let tx = self.db.transaction()?;
        self.setting.set_thermocouples(&tx, Some(thermocouples))?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_interp_method(&self, responder: Responder<InterpMethod>) {
        responder.respond(self.interp_method());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_interp_method(&mut self, interp_method: InterpMethod, responder: Responder<()>) {
        trace!(?interp_method);
        responder.respond(self.set_interp_method(interp_method));
    }

    fn set_interp_method(&mut self, interp_method: InterpMethod) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_interp_method(&tx, interp_method)?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_interp_frame(&self, frame_index: usize, responder: Responder<Array2<f64>>) {
        trace!(frame_index);
        match self.interpolator() {
            Ok(interpolator) => {
                spawn(move || responder.respond(interpolator.interp_frame(frame_index)));
            }
            Err(e) => responder.respond_err(e),
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_iteration_method(&self, responder: Responder<IterationMethod>) {
        responder.respond(self.setting.iteration_method(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_iteration_method(
        &mut self,
        iteration_method: IterationMethod,
        responder: Responder<()>,
    ) {
        trace!(?iteration_method);
        responder.respond(self.set_iteration_method(iteration_method));
    }

    fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> Result<()> {
        self.setting
            .set_iteration_method(&self.db, iteration_method)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_physical_param(&self, responder: Responder<PhysicalParam>) {
        responder.respond(self.setting.physical_param(&self.db));
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_gmax_temperature(&mut self, gmax_temperature: f64, responder: Responder<()>) {
        responder.respond(self.set_gmax_temperature(gmax_temperature));
    }

    fn set_gmax_temperature(&mut self, gmax_temperature: f64) -> Result<()> {
        trace!(gmax_temperature);
        self.setting
            .set_gmax_temperature(&self.db, gmax_temperature)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_solid_thermal_conductivity(
        &mut self,
        solid_thermal_conductivity: f64,
        responder: Responder<()>,
    ) {
        trace!(solid_thermal_conductivity);
        responder.respond(self.set_solid_thermal_conductivity(solid_thermal_conductivity));
    }

    fn set_solid_thermal_conductivity(&mut self, solid_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_conductivity(&self.db, solid_thermal_conductivity)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_solid_thermal_diffusivity(
        &mut self,
        solid_thermal_diffusivity: f64,
        responder: Responder<()>,
    ) {
        trace!(solid_thermal_diffusivity);
        responder.respond(self.set_solid_thermal_diffusivity(solid_thermal_diffusivity));
    }

    fn set_solid_thermal_diffusivity(&mut self, solid_thermal_diffusivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_diffusivity(&self.db, solid_thermal_diffusivity)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_characteristic_length(
        &mut self,
        characteristic_length: f64,
        responder: Responder<()>,
    ) {
        trace!(characteristic_length);
        responder.respond(self.set_characteristic_length(characteristic_length));
    }

    fn set_characteristic_length(&mut self, characteristic_length: f64) -> Result<()> {
        self.setting
            .set_characteristic_length(&self.db, characteristic_length)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_set_air_thermal_conductivity(
        &mut self,
        air_thermal_conductivity: f64,
        responder: Responder<()>,
    ) {
        trace!(air_thermal_conductivity);
        responder.respond(self.set_air_thermal_conductivity(air_thermal_conductivity));
    }

    fn set_air_thermal_conductivity(&mut self, air_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_air_thermal_conductivity(&self.db, air_thermal_conductivity)?;
        self.nu_data = None;

        self.reconcile();

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_solve_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.solve_controller.solve_progress());
    }

    #[instrument(level = "trace", skip_all)]
    pub fn on_get_nu(&self, edge_truncation: Option<(f64, f64)>, responder: Responder<NuView>) {
        trace!(?edge_truncation);
        let f = || {
            let nu_data = self
                .nu_data
                .as_ref()
                .ok_or_else(|| anyhow!("not solved yet"))
                .cloned()?;
            let plot_path = self.plot_path()?;
            Ok((nu_data, plot_path))
        };

        let (nu_data, plot_path) = match f() {
            Ok(ret) => ret,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };

        spawn(move || {
            let NuData { nu2, nu_nan_mean } = nu_data;
            let edge_truncation = edge_truncation.unwrap_or((nu_nan_mean * 0.6, nu_nan_mean * 2.0));
            match draw_area(plot_path, nu2.view(), edge_truncation) {
                Ok(nu_plot_base64) => responder.respond_ok_no_result_log(NuView {
                    nu2,
                    nu_nan_mean,
                    nu_plot_base64,
                    edge_truncation,
                }),
                Err(e) => responder.respond_err(e),
            }
        });
    }
}
