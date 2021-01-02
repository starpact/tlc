pub mod calculate;

use std::path::Path;
use std::time::Instant;

use ndarray::prelude::*;

use calculate::*;
use error::TLCResult;
use io::{read_config, ConfigParas};

pub fn cal<P: AsRef<Path>>(config_path: P) -> TLCResult<f32> {
    let t0 = Instant::now();

    let ConfigParas {
        video_path,
        daq_path,
        save_dir,
        start_frame,
        start_row,
        top_left_pos,
        region_shape,
        temp_column_num,
        thermocouple_pos,
        interp_method,
        filter_method,
        peak_temp,
        solid_thermal_conductivity,
        solid_thermal_diffusivity,
        characteristic_length,
        air_thermal_conductivity,
        h0,
        max_iter_num,
    } = read_config(config_path)?;

    let (frame_num, frame_rate, total_frames, total_rows) =
        io::get_metadata(&video_path, &daq_path, start_frame, start_row)?;
    println!(
        "frame_num: {}\nframe_rate: {}\ntotal_frames: {}\ttotal_rows: {}",
        frame_num, frame_rate, total_frames, total_rows
    );

    println!("read video...");
    let video_record = (start_frame, frame_num, &video_path);
    let region_record = (top_left_pos, region_shape);
    let mut g2d = io::read_video(video_record, region_record)?;
    let dt = 1. / frame_rate as f32;
    let t1 = Instant::now();
    println!("{:?}", t1.duration_since(t0));

    println!("read daq...");
    let temp_record = (start_row, frame_num, &temp_column_num, &daq_path);
    let t2d = io::read_daq(temp_record)?;
    let t2 = Instant::now();
    println!("{:?}", t2.duration_since(t1));

    println!("filtering...");
    preprocess::filtering(g2d.view_mut(), filter_method);
    let t3 = Instant::now();
    println!("{:?}", t3.duration_since(t2));

    println!("detect peak...");
    let peak_frames = preprocess::detect_peak(g2d.view());
    let t4 = Instant::now();
    println!("{:?}", t4.duration_since(t3));

    println!("interpolate...");
    let (interp_temps, query_index) = preprocess::interp(
        t2d.view(),
        &thermocouple_pos,
        interp_method,
        top_left_pos,
        region_shape,
    );
    let t5 = Instant::now();
    println!("{:?}", t5.duration_since(t4));

    println!("start solving...");
    let nus = solve::solve(
        peak_frames.view(),
        interp_temps.view(),
        query_index.view(),
        solid_thermal_conductivity,
        solid_thermal_diffusivity,
        characteristic_length,
        air_thermal_conductivity,
        dt,
        peak_temp,
        h0,
        max_iter_num,
    );
    let t6 = Instant::now();
    println!("{:?}", t6.duration_since(t5));
    println!("\ntotal time cost: {:?}\n", t6.duration_since(t0));

    let (nu_nan_mean, nan_ratio) = postprocess::cal_average(nus.view());
    println!("overall average Nu: {}", nu_nan_mean);
    println!("nan percent: {:.3}%", nan_ratio);

    println!("saving...");
    let (nu_path, plot_path) = io::get_save_path(&video_path, &save_dir)?;
    let mut nu2d = nus
        .into_shape(region_shape)
        .map_err(|err| err!(UnKnown, err))?;
    nu2d.invert_axis(Axis(0));

    postprocess::plot_nu(nu2d.view(), nu_nan_mean * 0.6, nu_nan_mean * 2., plot_path)?;

    io::save_nu(nu2d.view(), nu_path)?;

    Ok(nu_nan_mean)
}
