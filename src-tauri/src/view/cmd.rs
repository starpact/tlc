use serde::Deserialize;

use crate::cal::{
    preprocess::{FilterMethod, InterpMethod},
    solve::IterationMethod,
};

#[derive(Deserialize)]
#[serde(tag = "cmd")]
pub enum Cmd {
    LoadDefaultConfig {
        callback: String,
        error: String,
    },
    LoadConfig {
        config_path: String,
        callback: String,
        error: String,
    },
    GetRawG2d {
        callback: String,
        error: String,
    },
    GetFilterG2d {
        callback: String,
        error: String,
    },
    GetT2d {
        callback: String,
        error: String,
    },
    GetNu2d {
        callback: String,
        error: String,
    },
    GetNuAve {
        callback: String,
        error: String,
    },
    SetSaveDir {
        save_dir: String,
        callback: String,
        error: String,
    },
    SetVideoPath {
        video_path: String,
        callback: String,
        error: String,
    },
    SetDAQPath {
        daq_path: String,
        callback: String,
        error: String,
    },
    SetFilterMethod {
        filter_method: FilterMethod,
        callback: String,
        error: String,
    },
    SetInterpMethod {
        interp_method: InterpMethod,
        callback: String,
        error: String,
    },
    SetIterationMethod {
        iteration_method: IterationMethod,
        callback: String,
        error: String,
    },
    SetRegion {
        region: [usize; 4],
        callback: String,
        error: String,
    },
    SetRegulator {
        regulator: Vec<f32>,
        callback: String,
        error: String,
    },
    SetPeakTemp {
        peak_temp: f32,
        callback: String,
        error: String,
    },
    SetSolidThermalConductivity {
        solid_thermal_conductivity: f32,
        callback: String,
        error: String,
    },
    SetSolidThermalDiffusivity {
        solid_thermal_diffusivity: f32,
        callback: String,
        error: String,
    },
    SetAirThermalConductivity {
        air_thermal_conductivity: f32,
        callback: String,
        error: String,
    },
    SetCharacteristicLength {
        characteristic_length: f32,
        callback: String,
        error: String,
    },
    SetStartFrame {
        start_frame: usize,
        callback: String,
        error: String,
    },
    SetStartRow {
        start_row: usize,
        callback: String,
        error: String,
    },
    SetTempColumnNum {
        temp_column_num: Vec<usize>,
        callback: String,
        error: String,
    },
    SetThermocouplePos {
        thermocouple_pos: Vec<(i32, i32)>,
        callback: String,
        error: String,
    },
    SaveConfig {
        callback: String,
        error: String,
    },
    GetFrame {
        callback: String,
        error: String,
        frame_index: usize,
    },
    SaveNu {
        callback: String,
        error: String,
    },
    PlotNu {
        callback: String,
        error: String,
    },
    PlotTempsSingleFrame {
        callback: String,
        error: String,
    },
}
