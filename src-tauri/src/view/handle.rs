use std::sync::mpsc::Receiver;
use std::thread;

use serde::Serialize;
use solve::IterationMethod;
use tauri::api::rpc::format_callback_result;
use tauri::WebviewMut;

use super::cmd::{Cmd, Cmd::*};
use crate::cal::{error::TLCResult, *};
use crate::err;
use preprocess::{FilterMethod, InterpMethod};

pub fn init(rx: Receiver<(WebviewMut, Cmd)>) {
    thread::spawn(move || {
        let mut tlc_data = None;

        loop {
            let (wm, cmd) = rx.recv().unwrap();
            execute(&mut tlc_data, wm, cmd).unwrap();
        }
    });
}

fn execute(tlc_data: &mut Option<TLCData>, wm: WebviewMut, cmd: Cmd) -> TLCResult<()> {
    match cmd {
        LoadDefaultConfig { callback, error } => tlc_data
            .load_default_config()
            .dispatch(wm, callback, error)?,

        LoadConfig {
            config_path,
            callback,
            error,
        } => tlc_data
            .load_config(config_path)
            .dispatch(wm, callback, error)?,

        SetSaveDir {
            save_dir,
            callback,
            error,
        } => tlc_data
            .set_save_dir(save_dir)
            .dispatch(wm, callback, error)?,

        SaveConfig { callback, error } => tlc_data.save_config().dispatch(wm, callback, error)?,

        SetVideoPath {
            video_path,
            callback,
            error,
        } => tlc_data
            .set_video_path(video_path)
            .dispatch(wm, callback, error)?,

        SetDAQPath {
            daq_path,
            callback,
            error,
        } => tlc_data
            .set_daq_path(daq_path)
            .dispatch(wm, callback, error)?,

        SetStartFrame {
            start_frame,
            callback,
            error,
        } => tlc_data
            .set_start_frame(start_frame)
            .dispatch(wm, callback, error)?,

        SetStartRow {
            start_row,
            callback,
            error,
        } => tlc_data
            .set_start_row(start_row)
            .dispatch(wm, callback, error)?,

        SetPeakTemp {
            peak_temp,
            callback,
            error,
        } => tlc_data
            .set_peak_temp(peak_temp)
            .dispatch(wm, callback, error)?,

        SetSolidThermalConductivity {
            solid_thermal_conductivity,
            callback,
            error,
        } => tlc_data
            .set_solid_thermal_conductivity(solid_thermal_conductivity)
            .dispatch(wm, callback, error)?,

        SetSolidThermalDiffusivity {
            solid_thermal_diffusivity,
            callback,
            error,
        } => tlc_data
            .set_solid_thermal_diffusivity(solid_thermal_diffusivity)
            .dispatch(wm, callback, error)?,

        SetAirThermalConductivity {
            air_thermal_conductivity,
            callback,
            error,
        } => tlc_data
            .set_air_thermal_conductivity(air_thermal_conductivity)
            .dispatch(wm, callback, error)?,

        SetCharacteristicLength {
            characteristic_length,
            callback,
            error,
        } => tlc_data
            .set_characteristic_length(characteristic_length)
            .dispatch(wm, callback, error)?,

        SetRegulator {
            regulator,
            callback,
            error,
        } => {
            tlc_data
                .set_regulator(regulator)
                .dispatch(wm, callback, error)?;
        }

        SetFilterMethod {
            filter_method,
            callback,
            error,
        } => tlc_data
            .set_filter_method(filter_method)
            .dispatch(wm, callback, error)?,

        SetInterpMethod {
            interp_method,
            callback,
            error,
        } => tlc_data
            .set_interp_method(interp_method)
            .dispatch(wm, callback, error)?,

        SetIterationMethod {
            iteration_method,
            callback,
            error,
        } => tlc_data
            .set_iteration_method(iteration_method)
            .dispatch(wm, callback, error)?,

        SetRegion {
            region,
            callback,
            error,
        } => tlc_data.set_region(region).dispatch(wm, callback, error)?,

        SetTempColumnNum {
            temp_column_num,
            callback,
            error,
        } => tlc_data
            .set_temp_column_num(temp_column_num)
            .dispatch(wm, callback, error)?,

        SetThermocouplePos {
            thermocouple_pos,
            callback,
            error,
        } => tlc_data
            .set_thermocouple_pos(thermocouple_pos)
            .dispatch(wm, callback, error)?,

        ReadVideo { callback, error } => tlc_data.read_video().dispatch(wm, callback, error)?,

        ReadDAQ { callback, error } => tlc_data.read_daq().dispatch(wm, callback, error)?,

        _ => {}
    }

    Ok(())
}

trait Handle {
    fn get(&mut self) -> TLCResult<&mut TLCData>;
    fn load_config(&mut self, config_path: String) -> TLCResult<&TLCConfig>;
    fn load_default_config(&mut self) -> TLCResult<&TLCConfig>;
    fn save_config(&mut self) -> TLCResult<()>;
    fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&TLCConfig>;
    fn set_video_path(&mut self, video_path: String) -> TLCResult<&TLCConfig>;
    fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&TLCConfig>;
    fn set_start_frame(&mut self, start_frame: usize) -> TLCResult<&TLCConfig>;
    fn set_start_row(&mut self, start_row: usize) -> TLCResult<&TLCConfig>;
    fn set_peak_temp(&mut self, peak_temp: f32) -> TLCResult<&TLCConfig>;
    fn set_solid_thermal_conductivity(
        &mut self,
        solid_thermal_conductivity: f32,
    ) -> TLCResult<&TLCConfig>;
    fn set_solid_thermal_diffusivity(
        &mut self,
        solid_thermal_diffusivity: f32,
    ) -> TLCResult<&TLCConfig>;
    fn set_air_thermal_conductivity(
        &mut self,
        air_thermal_conductivity: f32,
    ) -> TLCResult<&TLCConfig>;
    fn set_characteristic_length(&mut self, characteristic_length: f32) -> TLCResult<&TLCConfig>;
    fn set_regulator(&mut self, regulator: Vec<f32>) -> TLCResult<&TLCConfig>;
    fn set_filter_method(&mut self, filter_method: FilterMethod) -> TLCResult<&TLCConfig>;
    fn set_interp_method(&mut self, interp_method: InterpMethod) -> TLCResult<&TLCConfig>;
    fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> TLCResult<&TLCConfig>;

    /// todo
    fn set_region(&mut self, region: [usize; 4]) -> TLCResult<&TLCConfig>;
    fn set_temp_column_num(&mut self, temp_column_num: Vec<usize>) -> TLCResult<&TLCConfig>;
    fn set_thermocouple_pos(&mut self, thermocouple: Vec<(i32, i32)>) -> TLCResult<&TLCConfig>;
    fn read_video(&mut self) -> TLCResult<()>;
    fn read_daq(&mut self) -> TLCResult<()>;
}

impl Handle for Option<TLCData> {
    fn get(&mut self) -> TLCResult<&mut TLCData> {
        Ok(self.get_or_insert(TLCData::new()?))
    }

    fn load_default_config(&mut self) -> TLCResult<&TLCConfig> {
        Ok(self.insert(TLCData::new()?).get_config())
    }

    fn load_config(&mut self, config_path: String) -> TLCResult<&TLCConfig> {
        Ok(self.insert(TLCData::from_path(config_path)?).get_config())
    }

    fn save_config(&mut self) -> TLCResult<()> {
        self.get_or_insert(TLCData::new()?).save_config()?;

        Ok(())
    }

    fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_save_dir(save_dir)?.get_config())
    }

    fn set_video_path(&mut self, video_path: String) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_video_path(video_path)?.get_config())
    }

    fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_daq_path(daq_path)?.get_config())
    }

    fn set_start_frame(&mut self, start_frame: usize) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_start_frame(start_frame).get_config())
    }

    fn set_start_row(&mut self, start_row: usize) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_start_row(start_row).get_config())
    }

    fn set_peak_temp(&mut self, peak_temp: f32) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_peak_temp(peak_temp).get_config())
    }

    fn set_solid_thermal_conductivity(
        &mut self,
        solid_thermal_conductivity: f32,
    ) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_solid_thermal_conductivity(solid_thermal_conductivity)
            .get_config())
    }

    fn set_solid_thermal_diffusivity(
        &mut self,
        solid_thermal_diffusivity: f32,
    ) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_solid_thermal_diffusivity(solid_thermal_diffusivity)
            .get_config())
    }

    fn set_air_thermal_conductivity(
        &mut self,
        air_thermal_conductivity: f32,
    ) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_air_thermal_conductivity(air_thermal_conductivity)
            .get_config())
    }

    fn set_characteristic_length(&mut self, characteristic_length: f32) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_characteristic_length(characteristic_length)
            .get_config())
    }

    fn set_regulator(&mut self, regulator: Vec<f32>) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_regulator(regulator).get_config())
    }

    fn set_filter_method(&mut self, filter_method: FilterMethod) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_filter_method(filter_method).get_config())
    }

    fn set_interp_method(&mut self, interp_method: InterpMethod) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_interp_method(interp_method).get_config())
    }

    fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_iteration_method(iteration_method)
            .get_config())
    }

    fn set_region(&mut self, region: [usize; 4]) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_region((region[0], region[1]), (region[2], region[3]))
            .get_config())
    }

    fn set_temp_column_num(&mut self, temp_column_num: Vec<usize>) -> TLCResult<&TLCConfig> {
        Ok(self
            .get()?
            .set_temp_column_num(temp_column_num)
            .get_config())
    }

    fn set_thermocouple_pos(&mut self, thermocouple: Vec<(i32, i32)>) -> TLCResult<&TLCConfig> {
        Ok(self.get()?.set_thermocouple_pos(thermocouple).get_config())
    }

    fn read_video(&mut self) -> TLCResult<()> {
        self.get()?
            .read_video()?
            .set_start_frame(84)
            .read_video()?
            .set_start_frame(84)
            .read_video()?
            .set_start_frame(84);

        Ok(())
    }

    fn read_daq(&mut self) -> TLCResult<()> {
        self.get()?.read_daq()?;

        Ok(())
    }
}

trait Dispatch {
    fn dispatch(self, wm: WebviewMut, callback: String, error: String) -> TLCResult<()>;
}

impl<T: Serialize> Dispatch for TLCResult<T> {
    fn dispatch(self, mut wm: WebviewMut, callback: String, error: String) -> TLCResult<()> {
        let callback_string =
            format_callback_result(self.map_err(|err| err.to_string()), callback, error)
                .map_err(|err| err!(err))?;
        println!("{}", callback_string);
        wm.dispatch(move |w| w.eval(callback_string.as_str()))
            .map_err(|err| err!(err))?;

        Ok(())
    }
}
