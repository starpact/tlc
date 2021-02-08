use core::panic;
use std::sync::mpsc::Receiver;
use std::thread;

use serde::Serialize;
use tauri::api::rpc::format_callback_result;
use tauri::WebviewMut;

use super::cmd::{Cmd, Cmd::*};
use crate::calculate::{error::TLCResult, *};
use crate::err;

pub fn init(rx: Receiver<(WebviewMut, Cmd)>) {
    thread::spawn(move || {
        let mut tlc_data = None;

        loop {
            match rx.recv() {
                Err(err) => panic!("{}", err),
                Ok((wm, cmd)) => match execute(&mut tlc_data, wm, cmd) {
                    Err(err) => panic!("{}", err),
                    Ok(_) => {}
                },
            }
        }
    });
}

fn execute(tlc_data: &mut Option<TLCData>, wm: WebviewMut, cmd: Cmd) -> TLCResult<()> {
    match cmd {
        GetConfig { callback, error } => {
            let config = get_config(tlc_data).map_err(|err| err.to_string());
            dispatch(config, wm, callback, error)?;
        }
        LoadConfig {
            config_path,
            callback,
            error,
        } => {
            let config = load_config(tlc_data, config_path).map_err(|err| err.to_string());
            dispatch(config, wm, callback, error)?;
        }
        SetVideoPath {
            video_path,
            callback,
            error,
        } => {
            let config = set_video_path(tlc_data, video_path).map_err(|err| err.to_string());
            dispatch(config, wm, callback, error)?;
        }
        _ => {}
    }

    Ok(())
}

fn get_config(tlc_data: &mut Option<TLCData>) -> TLCResult<&TLCConfig> {
    Ok(tlc_data.get_or_insert(TLCData::new()?).get_config())
}

fn load_config(tlc_data: &mut Option<TLCData>, config_path: String) -> TLCResult<&TLCConfig> {
    Ok(tlc_data
        .insert(TLCData::from_path(config_path)?)
        .get_config())
}

fn set_video_path(tlc_data: &mut Option<TLCData>, video_path: String) -> TLCResult<&TLCConfig> {
    let config = tlc_data
        .get_or_insert(TLCData::new()?)
        .set_video_path(video_path)?
        .get_config();

    Ok(config)
}

fn dispatch<T: Serialize>(
    res: Result<T, String>,
    mut wm: WebviewMut,
    callback: String,
    error: String,
) -> TLCResult<()> {
    let callback_string =
        format_callback_result(res, callback, error).map_err(|err| err!(ConfigError, err))?;
    println!("{}", callback_string);
    wm.dispatch(move |w| w.eval(callback_string.as_str()))
        .map_err(|err| err!(err))?;
    Ok(())
}
