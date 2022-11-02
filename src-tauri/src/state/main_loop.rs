use anyhow::Result;
use crossbeam::{channel::Receiver, select};
use tracing::error;

use super::GlobalState;
use crate::{request::Request, setting::new_db, state::outcome_handler::Outcome};

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

pub fn main_loop(request_receiver: Receiver<Request>) {
    let db = new_db(SQLITE_FILEPATH);
    let mut global_state = GlobalState::new(db);
    loop {
        if let Err(e) = global_state.handle(&request_receiver) {
            error!("{e}");
        }
    }
}

impl GlobalState {
    /// `handle` keeps receiving `Request`(frontend message) and `Outcome`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `outcome_sender`.
    fn handle(&mut self, request_receiver: &Receiver<Request>) -> Result<()> {
        select! {
            recv(request_receiver)  -> request => self.handle_request(request?),
            recv(self.outcome_receiver) -> outcome => self.handle_outcome(outcome?)?,
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) {
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

    fn handle_outcome(&mut self, outcome: Outcome) -> Result<()> {
        use Outcome::*;
        match outcome {
            ReadVideoMeta {
                video_id,
                video_meta,
                parameters,
            } => self.on_complete_read_video_meta(video_id, video_meta, parameters)?,
            LoadVideoPacket {
                video_id: video_meta,
                packet,
            } => {
                self.on_complete_load_video_packet(video_meta, packet)?;
            }
            ReadDaq {
                daq_id,
                daq_meta,
                daq_raw,
            } => self.on_complete_read_daq(daq_id, daq_meta, daq_raw)?,
            BuildGreen2 {
                green2_id: green2_meta,
                green2,
            } => self.on_complete_build_green2(green2_meta, green2)?,
            Interp {
                interp_id,
                interpolator,
            } => self.on_complete_interp(interp_id, interpolator)?,
            DetectPeak {
                gmax_id: gmax_meta,
                gmax_frame_indexes,
            } => self.on_complete_detect_peak(gmax_meta, gmax_frame_indexes)?,
            Solve {
                solve_id,
                nu2,
                nu_nan_mean,
            } => self.on_solve(solve_id, nu2, nu_nan_mean)?,
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossbeam::channel::{bounded, Sender};
    use tokio::sync::oneshot;
    use tracing::error;

    use super::*;
    use crate::{
        request::{Request, Responder, SettingData},
        setting::new_db_in_memory,
        solve::PhysicalParam,
    };

    #[test]
    fn test_main_loop_send_recv() {
        let sender = spawn_main_loop();

        let (tx, rx) = oneshot::channel();
        sender
            .send(Request::GetFilterMethod {
                responder: Responder::simple(tx),
            })
            .unwrap();
        let ret = rx.blocking_recv().unwrap().unwrap();
        println!("{ret:?}")
    }

    fn spawn_main_loop() -> Sender<Request> {
        let (request_sender, request_receiver) = bounded(3);
        let db = new_db_in_memory();
        let mut global_state = GlobalState::new(db);
        std::thread::spawn(move || loop {
            if let Err(e) = global_state.handle(&request_receiver) {
                error!("{e}");
            }
        });

        let (tx, rx) = oneshot::channel();
        request_sender
            .send(Request::CreateSetting {
                create_setting: Box::new(SettingData {
                    name: "test_case".to_owned(),
                    save_root_dir: PathBuf::from("fake_save_root_dir"),
                    video_path: None,
                    daq_path: None,
                    start_frame: None,
                    start_row: None,
                    area: None,
                    thermocouples: None,
                    interp_method: None,
                    filter_method: None,
                    iteration_method: None,
                    physical_param: PhysicalParam {
                        gmax_temperature: 35.48,
                        solid_thermal_conductivity: 0.19,
                        solid_thermal_diffusivity: 1.091e-7,
                        characteristic_length: 0.015,
                        air_thermal_conductivity: 0.0276,
                    },
                }),
                responder: Responder::simple(tx),
            })
            .unwrap();
        rx.blocking_recv().unwrap().unwrap();

        request_sender
    }
}
