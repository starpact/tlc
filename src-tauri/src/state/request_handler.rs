use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::{ArcArray2, Array2};
use tlc_video::{filter_point, read_video, FilterMethod, Progress, ProgressBar, VideoMeta};
use tracing::{error, info_span, warn};

use super::{GlobalState, Outcome};
use crate::{
    daq::{read_daq, DaqMeta, InterpMethod, Thermocouple},
    post_processing::draw_area,
    request::{NuView, Request, Responder, SettingData},
    setting::StartIndex,
    solve::{IterationMethod, NuData, PhysicalParam},
};

impl GlobalState {
    pub fn handle_request(&mut self, request: Request) {
        use Request::*;
        match request {
            CreateSetting {
                create_setting,
                responder,
            } => self.on_create_setting(create_setting, responder),
            SwitchSetting {
                setting_id,
                responder,
            } => self.on_switch_setting(setting_id, responder),
            DeleteSetting {
                setting_id,
                responder,
            } => self.on_delete_setting(setting_id, responder),
            GetName { responder } => self.on_get_name(responder),
            SetName { name, responder } => self.on_set_name(name, responder),
            GetSaveRootDir { responder } => self.on_get_save_root_dir(responder),
            SetSaveRootDir {
                save_root_dir,
                responder,
            } => self.on_set_save_root_dir(save_root_dir, responder),
            GetVideoMeta { responder } => self.on_get_video_meta(responder),
            SetVideoPath {
                video_path,
                responder,
            } => self.on_set_video_path(video_path, responder),
            GetReadVideoProgress { responder } => self.on_get_read_video_progress(responder),
            DecodeFrameBase64 {
                frame_index,
                responder,
            } => self.on_decode_frame_base64(frame_index, responder),
            GetDaqMeta { responder } => self.on_get_daq_meta(responder),
            SetDaqPath {
                daq_path,
                responder,
            } => self.on_set_daq_path(daq_path, responder),
            GetDaqRaw { responder } => self.on_get_daq_raw(responder),
            GetStartIndex { responder } => self.on_get_start_index(responder),
            SynchronizeVideoAndDaq {
                start_frame,
                start_row,
                responder,
            } => self.on_synchronize_video_and_daq(start_frame, start_row, responder),
            SetStartFrame {
                start_frame,
                responder,
            } => self.on_set_start_frame(start_frame, responder),
            SetStartRow {
                start_row,
                responder,
            } => self.on_set_start_row(start_row, responder),
            GetArea { responder } => self.on_get_area(responder),
            SetArea { area, responder } => self.on_set_area(area, responder),
            GetBuildGreen2Progress { responder } => self.on_get_build_green2_progress(responder),
            GetFilterMethod { responder } => self.on_get_filter_method(responder),
            SetFilterMethod {
                filter_method,
                responder,
            } => self.on_set_filter_method(filter_method, responder),
            GetDetectPeakProgress { responder } => self.on_get_detect_peak_progress(responder),
            FilterPoint {
                position,
                responder,
            } => self.on_filter_point(position, responder),
            GetThermocouples { responder } => self.on_get_thermocouples(responder),
            SetThermocouples {
                thermocouples,
                responder,
            } => self.on_set_thermocouples(thermocouples, responder),
            GetInterpMethod { responder } => self.on_get_interp_method(responder),
            SetInterpMethod {
                interp_method,
                responder,
            } => self.on_set_interp_method(interp_method, responder),
            InterpFrame {
                frame_index,
                responder,
            } => self.on_interp_frame(frame_index, responder),
            GetIterationMethod { responder } => self.on_get_iteration_method(responder),
            SetIterationMethod {
                iteration_method,
                responder,
            } => self.on_set_iteration_method(iteration_method, responder),
            GetPhysicalParam { responder } => self.on_get_physical_param(responder),
            SetGmaxTemperature {
                gmax_temperature,
                responder,
            } => self.on_set_gmax_temperature(gmax_temperature, responder),
            SetSolidThermalConductivity {
                solid_thermal_conductivity,
                responder,
            } => self.on_set_solid_thermal_conductivity(solid_thermal_conductivity, responder),
            SetSolidThermalDiffusivity {
                solid_thermal_diffusivity,
                responder,
            } => self.on_set_solid_thermal_diffusivity(solid_thermal_diffusivity, responder),
            SetCharacteristicLength {
                characteristic_length,
                responder,
            } => self.on_set_characteristic_length(characteristic_length, responder),
            SetAirThermalConductivity {
                air_thermal_conductivity,
                responder,
            } => self.on_set_air_thermal_conductivity(air_thermal_conductivity, responder),
            GetNu {
                edge_truncation,
                responder,
            } => self.on_get_nu(edge_truncation, responder),
        }
    }

    pub fn on_create_setting(
        &mut self,
        create_setting: Box<SettingData>,
        responder: Responder<()>,
    ) {
        responder.respond(
            self.setting
                .create_setting(&self.db, (*create_setting).into()),
        );
        // TODO
    }

    pub fn on_switch_setting(&mut self, setting_id: i64, responder: Responder<()>) {
        responder.respond(self.setting.switch_setting(&self.db, setting_id));
        // TODO
    }

    pub fn on_delete_setting(&mut self, setting_id: i64, responder: Responder<()>) {
        responder.respond(self.setting.delete_setting(&self.db, setting_id));
        // TODO
    }

    pub fn on_get_name(&self, responder: Responder<String>) {
        responder.respond(self.setting.name(&self.db));
    }

    pub fn on_set_name(&self, name: String, responder: Responder<()>) {
        responder.respond(self.setting.set_name(&self.db, &name));
    }

    pub fn on_get_save_root_dir(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting.save_root_dir(&self.db));
    }

    pub fn on_set_save_root_dir(&mut self, save_root_dir: PathBuf, responder: Responder<()>) {
        responder.respond(self.setting.set_save_root_dir(&self.db, &save_root_dir));
    }

    pub fn on_get_video_meta(&self, responder: Responder<VideoMeta>) {
        responder.respond(self.video_meta())
    }

    pub fn on_set_video_path(&mut self, video_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.set_video_path(&video_path) {
            responder.respond_err(e);
            return;
        }

        let progress_bar = self.video_controller.prepare_read_video();
        self.spawn(|outcome_sender| {
            do_read_video(video_path, responder, outcome_sender, progress_bar);
        });
    }

    fn set_video_path(&mut self, video_path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_video_path(&tx, video_path)?;
        self.video_data = None;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_get_read_video_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.read_video_progress());
    }

    pub fn on_decode_frame_base64(&self, frame_index: usize, responder: Responder<String>) {
        let f = || {
            let video_data = self.video_data()?;
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

        std::thread::spawn(move || responder.respond(decoder_manager.decode_frame_base64(packet)));
    }

    pub fn on_get_daq_meta(&self, responder: Responder<DaqMeta>) {
        responder.respond(self.daq_meta());
    }

    pub fn on_set_daq_path(&mut self, daq_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.set_daq_path(&daq_path) {
            responder.respond_err(e);
            return;
        }

        self.spawn(|outcome_sender| match read_daq(daq_path) {
            Ok((daq_meta, daq_raw)) => {
                outcome_sender
                    .send(Outcome::ReadDaq { daq_meta, daq_raw })
                    .unwrap();
                responder.respond_ok(());
            }
            Err(e) => responder.respond_err(e),
        });
    }

    fn set_daq_path(&mut self, daq_path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_daq_path(&tx, daq_path)?;
        self.daq_data = None;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_get_daq_raw(&self, responder: Responder<ArcArray2<f64>>) {
        responder.respond(self.daq_raw())
    }

    pub fn on_get_start_index(&self, responder: Responder<StartIndex>) {
        responder.respond(self.start_index());
    }

    pub fn on_synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
        responder: Responder<()>,
    ) {
        let ret = self.synchronize_video_and_daq(start_frame, start_row);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
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

        let tx = self.db.transaction()?;
        self.setting.set_start_index(
            &tx,
            StartIndex {
                start_frame,
                start_row,
            },
        )?;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        tx.commit()?;

        Ok(())
    }

    pub fn on_set_start_frame(&mut self, start_frame: usize, responder: Responder<()>) {
        let ret = self.set_start_frame(start_frame);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
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

        let tx = self.db.transaction()?;
        self.setting.set_start_index(
            &tx,
            StartIndex {
                start_frame,
                start_row,
            },
        )?;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_set_start_row(&mut self, start_row: usize, responder: Responder<()>) {
        let ret = self.set_start_row(start_row);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
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

        let tx = self.db.transaction()?;
        self.setting.set_start_index(
            &tx,
            StartIndex {
                start_frame,
                start_row,
            },
        )?;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }
    pub fn on_get_area(&self, responder: Responder<(u32, u32, u32, u32)>) {
        responder.respond(self.area());
    }

    pub fn on_set_area(&mut self, area: (u32, u32, u32, u32), responder: Responder<()>) {
        let ret = self.set_area(area);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
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

        let tx = self.db.transaction()?;
        self.setting.set_area(&tx, area)?;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_get_build_green2_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.build_green2_progress());
    }

    pub fn on_get_filter_method(&self, responder: Responder<FilterMethod>) {
        responder.respond(self.setting.filter_method(&self.db));
    }

    pub fn on_set_filter_method(&mut self, filter_method: FilterMethod, responder: Responder<()>) {
        let ret = self.set_filter_method(filter_method);
        if ret.is_ok() {
            let _ = self.spawn_detect_peak();
        }
        responder.respond(ret);
    }

    fn set_filter_method(&mut self, filter_method: FilterMethod) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_filter_method(&tx, filter_method)?;
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_gmax_frame_indexes(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_get_detect_peak_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.detect_peak_progress());
    }

    pub fn on_filter_point(&self, position: (usize, usize), responder: Responder<Vec<u8>>) {
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

        std::thread::spawn(move || {
            responder.respond(filter_point(green2, filter_method, area, position))
        });
    }

    pub fn on_get_thermocouples(&self, responder: Responder<Vec<Thermocouple>>) {
        responder.respond(self.thermocouples());
    }

    pub fn on_set_thermocouples(
        &mut self,
        thermocouples: Vec<Thermocouple>,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_thermocouples(&thermocouples));
    }

    fn set_thermocouples(&mut self, thermocouples: &[Thermocouple]) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_thermocouples(&tx, thermocouples)?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        let _ = self.spawn_interp();

        Ok(())
    }

    pub fn on_get_interp_method(&self, responder: Responder<InterpMethod>) {
        responder.respond(self.interp_method());
    }

    pub fn on_set_interp_method(&mut self, interp_method: InterpMethod, responder: Responder<()>) {
        let ret = self.set_interp_method(interp_method);
        if ret.is_ok() {
            let _ = self.spawn_interp();
        }
        responder.respond(ret);
    }

    fn set_interp_method(&mut self, interp_method: InterpMethod) -> Result<()> {
        let mut interp_meta = self.interp_meta()?;
        if interp_meta.interp_method == interp_method {
            warn!("interp method unchanged, compute again anyway");
        } else {
            interp_meta.interp_method = interp_method;
        }

        let tx = self.db.transaction()?;
        self.setting.set_interp_method(&tx, interp_method)?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    pub fn on_interp_frame(&self, frame_index: usize, responder: Responder<Array2<f64>>) {
        match self.interpolator() {
            Ok(interpolator) => {
                std::thread::spawn(move || {
                    responder.respond(interpolator.interp_frame(frame_index))
                });
            }
            Err(e) => responder.respond_err(e),
        }
    }

    pub fn on_get_iteration_method(&self, responder: Responder<IterationMethod>) {
        responder.respond(self.setting.iteration_method(&self.db));
    }

    pub fn on_set_iteration_method(
        &mut self,
        iteration_method: IterationMethod,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_iteration_method(iteration_method));
    }

    fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> Result<()> {
        self.setting
            .set_iteration_method(&self.db, iteration_method)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    pub fn on_get_physical_param(&self, responder: Responder<PhysicalParam>) {
        responder.respond(self.setting.physical_param(&self.db));
    }

    pub fn on_set_gmax_temperature(&mut self, gmax_temperature: f64, responder: Responder<()>) {
        responder.respond(self.set_gmax_temperature(gmax_temperature));
    }

    fn set_gmax_temperature(&mut self, gmax_temperature: f64) -> Result<()> {
        self.setting
            .set_gmax_temperature(&self.db, gmax_temperature)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    pub fn on_set_solid_thermal_conductivity(
        &mut self,
        solid_thermal_conductivity: f64,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_solid_thermal_conductivity(solid_thermal_conductivity));
    }

    fn set_solid_thermal_conductivity(&mut self, solid_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_conductivity(&self.db, solid_thermal_conductivity)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    pub fn on_set_solid_thermal_diffusivity(
        &mut self,
        solid_thermal_diffusivity: f64,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_solid_thermal_diffusivity(solid_thermal_diffusivity));
    }

    pub fn on_set_characteristic_length(
        &mut self,
        characteristic_length: f64,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_characteristic_length(characteristic_length));
    }

    fn set_characteristic_length(&mut self, characteristic_length: f64) -> Result<()> {
        self.setting
            .set_characteristic_length(&self.db, characteristic_length)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    pub fn on_set_air_thermal_conductivity(
        &mut self,
        air_thermal_conductivity: f64,
        responder: Responder<()>,
    ) {
        responder.respond(self.set_air_thermal_conductivity(air_thermal_conductivity));
    }

    fn set_air_thermal_conductivity(&mut self, air_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_air_thermal_conductivity(&self.db, air_thermal_conductivity)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    pub fn on_get_nu(&self, edge_truncation: Option<(f64, f64)>, responder: Responder<NuView>) {
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

        std::thread::spawn(move || {
            let NuData { nu2, nu_nan_mean } = nu_data;
            let edge_truncation = edge_truncation.unwrap_or((nu_nan_mean * 0.6, nu_nan_mean * 2.0));
            match draw_area(plot_path, nu2.view(), edge_truncation) {
                Ok(nu_plot_base64) => responder.respond_ok(NuView {
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

fn do_read_video(
    video_path: PathBuf,
    responder: Responder<()>,
    outcome_sender: Sender<Outcome>,
    progress_bar: ProgressBar,
) {
    let (video_meta, parameters, packet_rx) = match read_video(video_path, progress_bar) {
        Ok(ret) => ret,
        Err(e) => {
            responder.respond_err(e);
            return;
        }
    };

    let nframes = video_meta.nframes;
    outcome_sender
        .send(Outcome::ReadVideoMeta {
            video_meta: video_meta.clone(),
            parameters,
        })
        .unwrap();
    responder.respond_ok(());

    let _span = info_span!("receive_loaded_packets", nframes).entered();
    let video_meta = Arc::new(video_meta);
    for cnt in 1.. {
        match packet_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(packet) => outcome_sender
                .send(Outcome::LoadVideoPacket {
                    video_meta: video_meta.clone(),
                    packet,
                })
                .unwrap(),
            Err(e) => {
                match e {
                    RecvTimeoutError::Timeout => error!("load packets got stuck for some reason"),
                    RecvTimeoutError::Disconnected => debug_assert_eq!(cnt, nframes),
                }
                return;
            }
        }
        debug_assert!(cnt < nframes);
    }
}
