use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Cmd {
    GetConfig,
    GetRawG2d,
    GetFilterG2d,
    GetT2d,
    GetNu2d,
    GetNuAve,

    SetSaveDir,
    SetVideoPath,
    SetDaqPath,
    SetFilterMethod,
    SetInterpMethod,
    SetIterationMethod,
    SetRegion,
    SetRegulator,
    SetSolidThermalConductivity,
    SetSolidThermalDiffusivity,
    SetAirThermalConductivity,
    SetCharacteristicLength,
    SetStartFrame,
    SetStartRow,
    SetTempColumnNum,
    SetThermocouple,

    ReadVideo,
    ReadDaq,

    SaveConfig,
    SaveNu,

    PlotNu,
    PlotTempsSingleFrame,
}
