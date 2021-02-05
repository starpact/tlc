#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::mpsc;
use tlc::view::cmd::Cmd::*;

fn main() {
    let (tx, rx) = mpsc::channel();
    tlc::view::handle::init(rx).unwrap();

    let app = tauri::AppBuilder::new();

    app.invoke_handler(move |webview, arg| {
        let webview_mut = webview.as_mut();

        match serde_json::from_str(arg) {
            Err(e) => Err(e.to_string()),
            Ok(command) => {
                match command {
                    GetConfig => {
                        tx.send(1).unwrap();
                    }
                    GetRawG2d => {}
                    GetFilterG2d => {}
                    GetT2d => {}
                    GetNu2d => {}
                    GetNuAve => {}
                    SetSaveDir => {}
                    SetVideoPath => {}
                    SetDaqPath => {}
                    SetFilterMethod => {}
                    SetInterpMethod => {}
                    SetIterationMethod => {}
                    SetRegion => {}
                    SetRegulator => {}
                    SetSolidThermalConductivity => {}
                    SetSolidThermalDiffusivity => {}
                    SetAirThermalConductivity => {}
                    SetCharacteristicLength => {}
                    SetStartFrame => {}
                    SetStartRow => {}
                    SetTempColumnNum => {}
                    SetThermocouple => {}
                    ReadVideo => {}
                    ReadDaq => {}
                    SaveConfig => {}
                    SaveNu => {}
                    PlotNu => {}
                    PlotTempsSingleFrame => {}
                }
                Ok(())
            }
        }
    })
    .build()
    .run();
}
