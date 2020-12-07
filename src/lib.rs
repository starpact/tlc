pub mod calculate;

use calculate::*;
use std::error::Error;
use std::path::Path;
use std::time::Instant;

pub fn cal<P: AsRef<Path>>(config_path: P) -> Result<(), Box<dyn Error>> {
    let t0 = Instant::now();

    let io::ConfigParas {
        video_path,
        daq_path,
        start_frame,
        start_row,
        upper_left_pos,
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
    } = io::read_config(config_path)?;

    let (frame_num, frame_rate, total_frames, total_rows) =
        io::get_metadata(&video_path, &daq_path, start_frame, start_row)?;
    println!(
        "frame_num: {}\nframe_rate: {}\ntotal_frames: {}\ttotal_rows: {}",
        frame_num, frame_rate, total_frames, total_rows
    );

    println!("read video...");
    let video_record = (start_frame, frame_num, video_path);
    let region_record = (upper_left_pos, region_shape);
    let g2d = io::read_video(video_record, region_record)?;
    let dt = 1. / frame_rate as f64;
    let t1 = Instant::now();
    println!("{:?}", t1.duration_since(t0));

    println!("read daq...");
    let temp_record = (start_row, frame_num, &temp_column_num, &daq_path);
    let t2d = io::read_daq(temp_record)?;
    let t2 = Instant::now();
    println!("{:?}", t2.duration_since(t1));

    println!("filtering...");
    let g2d_filtered = preprocess::filtering(g2d, filter_method);
    let t3 = Instant::now();
    println!("{:?}", t3.duration_since(t2));

    println!("detect peak...");
    let peak_frames = preprocess::detect_peak(g2d_filtered);
    let t4 = Instant::now();
    println!("{:?}", t4.duration_since(t3));

    println!("interpolate...");
    let (interp_temps, query_index) = preprocess::interp(
        t2d.view(),
        &thermocouple_pos,
        interp_method,
        upper_left_pos,
        region_shape,
    );
    let t5 = Instant::now();
    println!("{:?}", t5.duration_since(t4));

    println!("start solving...");
    let nus = solve::solve(
        solid_thermal_conductivity,
        solid_thermal_diffusivity,
        characteristic_length,
        air_thermal_conductivity,
        dt,
        peak_temp,
        peak_frames,
        interp_temps,
        query_index,
        h0,
        max_iter_num,
    );
    let t6 = Instant::now();
    println!("{:?}", t6.duration_since(t5));

    println!("\ntotal time cost: {:?}\n", t6.duration_since(t0));

    let (valid_count, valid_sum) = nus.iter().fold((0, 0.), |(count, sum), &h| {
        if h.is_finite() {
            (count + 1, sum + h)
        } else {
            (count, sum)
        }
    });
    println!("overall average Nu: {}", valid_sum / valid_count as f64);

    Ok(())
}
