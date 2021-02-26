use std::thread;
use std::{collections::HashMap, sync::mpsc::Receiver};

use tauri::WebviewMut;

use super::request::{Request, Value};
use crate::awsl;
use crate::cal::{error::TLCResult, *};

macro_rules! register {
    ($hm:expr, $($f:expr),* $(,)*) => {
        $($hm.insert(
            $crate::view::handle::snake_to_camel(stringify!($f)),
            &$f as &(dyn Fn(&mut TLCData, Request) -> TLCResult<String>)
        );)*
    };
}

fn snake_to_camel(snake: &str) -> String {
    let mut flag = false;
    let mut arr = Vec::with_capacity(snake.len());
    snake.bytes().for_each(|b| match (b, flag) {
        (b'_', _) => flag = true,
        (_, true) => {
            arr.push(b - 32);
            flag = false;
        }
        _ => arr.push(b),
    });

    String::from_utf8(arr).unwrap_or_default()
}

pub fn init(rx: Receiver<(WebviewMut, Request)>) {
    thread::spawn(move || {
        // 注册所有请求（表驱动）
        let mut hm = HashMap::new();
        register!(
            hm,
            load_default_config,
            load_config,
            save_config,
            set_save_dir,
            set_video_path,
            set_daq_path,
            set_start_frame,
            set_start_row,
            set_peak_temp,
            set_solid_thermal_conductivity,
            set_solid_thermal_diffusivity,
            set_air_thermal_conductivity,
            set_characteristic_length,
            set_regulator,
            set_filter_method,
            set_interp_method,
            set_iteration_method,
            set_region,
            set_temp_column_num,
            set_thermocouple_pos,
            get_frame,
        );

        let mut tlc_data = TLCData::new().unwrap();

        loop {
            let (mut wm, req) = rx.recv().unwrap();
            let f = hm.get(req.cmd.as_str()).unwrap();
            let callback_string = f(&mut tlc_data, req).unwrap();
            wm.dispatch(move |w| w.eval(callback_string.as_str()))
                .unwrap();
        }
    });
}

fn load_default_config(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = TLCData::new().map(|new_data| {
        *data = new_data;
        data.get_config()
    });

    Request::format_callback(res, req.callback, req.error)
}

fn load_config(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::String(config_path)) => TLCData::from_path(config_path).map(|new_data| {
            *data = new_data;
            data.get_config()
        }),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn save_config(data: &mut TLCData, req: Request) -> TLCResult<String> {
    Request::format_callback(
        data.save_config().map(|data| data.get_config()),
        req.callback,
        req.error,
    )
}

fn set_save_dir(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::String(save_dir)) => data.set_save_dir(save_dir).map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_video_path(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::String(video_path)) => data
            .set_video_path(video_path)
            .map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_daq_path(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::String(daq_path)) => data.set_daq_path(daq_path).map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_start_frame(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Uint(start_frame)) => Ok(data.set_start_frame(start_frame).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_start_row(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Uint(start_row)) => Ok(data.set_start_row(start_row).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_peak_temp(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Float(peak_temp)) => Ok(data.set_peak_temp(peak_temp).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_solid_thermal_conductivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Float(solid_thermal_conductivity)) => Ok(data
            .set_solid_thermal_conductivity(solid_thermal_conductivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_solid_thermal_diffusivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Float(solid_thermal_diffusivity)) => Ok(data
            .set_solid_thermal_diffusivity(solid_thermal_diffusivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_air_thermal_conductivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Float(air_thermal_conductivity)) => Ok(data
            .set_air_thermal_conductivity(air_thermal_conductivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_characteristic_length(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Float(characteristic_length)) => Ok(data
            .set_characteristic_length(characteristic_length)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_regulator(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::FloatVec(regulator)) => Ok(data.set_regulator(regulator).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_filter_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Filter(filter_method)) => {
            Ok(data.set_filter_method(filter_method).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_interp_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Interp(interp_method)) => {
            Ok(data.set_interp_method(interp_method).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_iteration_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Iteration(iteration_method)) => {
            Ok(data.set_iteration_method(iteration_method).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_region(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::UintVec(region)) if region.len() == 4 => Ok(data
            .set_region((region[0], region[1]), (region[2], region[3]))
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_temp_column_num(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::UintVec(temp_column_num)) => {
            Ok(data.set_temp_column_num(temp_column_num).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_thermocouple_pos(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::IntPairVec(thermocouple_pos)) => {
            Ok(data.set_thermocouple_pos(thermocouple_pos).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn get_frame(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Some(Value::Uint(frame_index)) => data.get_frame(frame_index),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}
