use std::thread;
use std::{collections::HashMap, sync::mpsc::Receiver};

use ndarray::ArrayView2;
use tauri::WebviewMut;

use super::request::{Request, Value};
use crate::awsl;
use crate::cal::{error::TLCResult, *};

macro_rules! register {
    (@$hm:expr, $($f:expr),* $(,)*) => {
        $($hm.insert(
            snake_to_camel(stringify!($f)),
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
            @hm,
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
            set_thermocouples,
            get_frame,
            get_daq,
            synchronize,
            get_interp_single_frame,
            try_drop_video,
            get_green_history,
            get_point_nu,
            set_color_range,
        );

        let mut tlc_data = TLCData::new();

        loop {
            let (mut wm, req) = rx.recv().unwrap();
            let f = match hm.get(req.cmd.as_str()) {
                Some(f) => f,
                None => continue, // won't happen
            };
            let callback_string = match tlc_data.as_mut() {
                Ok(tlc_data) => f(tlc_data, req),
                Err(err) => {
                    let res: Result<(), String> = Err(err.to_string());
                    Request::format_callback(res, req.callback, req.error)
                }
            };
            if let Ok(callback_string) = callback_string {
                let _ = wm.dispatch(move |w| w.eval(callback_string.as_str()));
            }
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
        Value::String(config_path) => TLCData::from_path(config_path).map(|new_data| {
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
        Value::String(save_dir) => data.set_save_dir(save_dir).map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_video_path(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::String(video_path) => data
            .set_video_path(video_path)
            .map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_daq_path(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::String(daq_path) => data.set_daq_path(daq_path).map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_start_frame(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Uint(start_frame) => data
            .set_start_frame(start_frame)
            .map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_start_row(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Uint(start_row) => data.set_start_row(start_row).map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_peak_temp(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Float(peak_temp) => Ok(data.set_peak_temp(peak_temp).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_solid_thermal_conductivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Float(solid_thermal_conductivity) => Ok(data
            .set_solid_thermal_conductivity(solid_thermal_conductivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_solid_thermal_diffusivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Float(solid_thermal_diffusivity) => Ok(data
            .set_solid_thermal_diffusivity(solid_thermal_diffusivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_air_thermal_conductivity(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Float(air_thermal_conductivity) => Ok(data
            .set_air_thermal_conductivity(air_thermal_conductivity)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_characteristic_length(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Float(characteristic_length) => Ok(data
            .set_characteristic_length(characteristic_length)
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_regulator(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::FloatVec(regulator) => Ok(data.set_regulator(regulator).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_filter_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Filter(filter_method) => Ok(data.set_filter_method(filter_method).get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_interp_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Interp(interp_method) => data
            .set_interp_method(interp_method)
            .map(|data| data.get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_iteration_method(data: &mut TLCData, req: Request) -> TLCResult<String> {
    fn f(data: &mut TLCData, body: Value) -> TLCResult<(String, f32)> {
        match body {
            Value::Iteration(iteration_method) => {
                data.set_iteration_method(iteration_method)
                    .solve()?
                    .save_nu()?;
                let nu2d_string = data.get_nu_img(None)?;
                let nu_nan_mean = data.get_nu_nan_mean()?;
                Ok((nu2d_string, nu_nan_mean))
            }
            _ => Err(awsl!(body)),
        }
    }

    Request::format_callback(f(data, req.body), req.callback, req.error)
}

fn set_region(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::UintVec(region) if region.len() == 4 => Ok(data
            .set_region((region[0], region[1]), (region[2], region[3]))
            .get_config()),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_thermocouples(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Thermocouples(thermocouples) => {
            Ok(data.set_thermocouples(thermocouples).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn get_frame(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Uint(frame_index) => data.get_frame(frame_index),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn get_daq(data: &mut TLCData, req: Request) -> TLCResult<String> {
    fn f(data: &mut TLCData) -> TLCResult<ArrayView2<f32>> {
        if data.get_daq().is_err() {
            data.read_daq()?;
        }
        data.get_daq()
    }

    Request::format_callback(f(data), req.callback, req.error)
}

fn synchronize(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::UintVec(arr) => {
            let (frame_index, row_index) = (arr[0], arr[1]);
            Ok(data.synchronize(frame_index, row_index).get_config())
        }
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn get_interp_single_frame(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Uint(frame_index) => data.interp_single_frame(frame_index),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

/// 如果当前Green矩阵已存在，则说明不需要重新解码视频，可以将视频缓存数据包和解码相关内存析构
fn try_drop_video(data: &mut TLCData, req: Request) -> TLCResult<String> {
    if let Ok(_) = data.get_raw_g2d() {
        data.drop_video();
    }

    Request::format_callback(TLCResult::Ok(()), req.callback, req.error)
}

fn get_green_history(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::Uint(pos) => data.filtering_single_point(pos),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn get_point_nu(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::UintVec(pos) => data.get_nu2d().map(|nu2d| nu2d.row(pos[0])[pos[1]]),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}

fn set_color_range(data: &mut TLCData, req: Request) -> TLCResult<String> {
    let res = match req.body {
        Value::FloatVec(range) => data.get_nu_img(Some((range[0], range[1]))),
        _ => Err(awsl!(req.body)),
    };

    Request::format_callback(res, req.callback, req.error)
}
