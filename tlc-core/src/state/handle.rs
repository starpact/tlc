use crossbeam::{
    channel::{Receiver, RecvError},
    select,
};

use crate::{
    request::Request,
    state::{GlobalState, Output},
};

impl GlobalState {
    /// `handle` keeps receiving `Request`(frontend message) and `Output`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `output_sender`.
    pub fn handle(
        &mut self,
        request_receiver: &Receiver<Request>,
    ) -> core::result::Result<(), RecvError> {
        select! {
            recv(request_receiver) -> request => self.handle_request(request?),
            recv(self.output_receiver) -> output => self.handle_output(output?),
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
            GetVideoPath { responder } => self.on_get_video_path(responder),
            SetVideoPath {
                video_path,
                responder,
            } => self.on_set_video_path(video_path, responder),
            GetVideoMeta { responder } => self.on_get_video_meta(responder),
            GetReadVideoProgress { responder } => self.on_get_read_video_progress(responder),
            DecodeFrameBase64 {
                frame_index,
                responder,
            } => self.on_decode_frame_base64(frame_index, responder),
            GetDaqPath { responder } => self.on_get_daq_path(responder),
            SetDaqPath {
                daq_path,
                responder,
            } => self.on_set_daq_path(daq_path, responder),
            GetDaqMeta { responder } => self.on_get_daq_meta(responder),
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
            GetSolveProgress { responder } => self.on_get_solve_progress(responder),
            GetNu {
                edge_truncation,
                responder,
            } => self.on_get_nu(edge_truncation, responder),
        }
    }

    fn handle_output(&mut self, output: Output) {
        use Output::*;
        let _ = match output {
            ReadVideoMeta {
                video_id,
                video_meta,
                parameters,
            } => self.on_complete_read_video_meta(video_id, video_meta, parameters),
            LoadVideoPacket {
                video_id: video_meta,
                packet,
            } => self.on_complete_load_video_packet(video_meta, packet),
            ReadDaq { daq_id, daq_raw } => self.on_complete_read_daq(daq_id, daq_raw),
            BuildGreen2 {
                green2_id: green2_meta,
                green2,
            } => self.on_complete_build_green2(green2_meta, green2),
            Interp {
                interp_id,
                interpolator,
            } => self.on_complete_interp(interp_id, interpolator),
            DetectPeak {
                gmax_id: gmax_meta,
                gmax_frame_indexes,
            } => self.on_complete_detect_peak(gmax_meta, gmax_frame_indexes),
            Solve {
                solve_id,
                nu2,
                nu_nan_mean,
            } => self.on_complete_solve(solve_id, nu2, nu_nan_mean),
        };
    }
}
