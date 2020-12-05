use std::time::Instant;
use tlc::calculate::*;

const CONFIG_PATH: &str = "./config/config.json";

fn main() {
    let t0 = Instant::now();

    let io::ConfigParas {
        start_frame,
        frame_num,
        video_path,
        upper_left_pos,
        region_shape,
        start_line,
        temp_column_num,
        excel_path,
        filter_method,
        interp_method,
        thermocouple_pos,
        solid_thermal_conductivity,
        solid_thermal_diffusivity,
        characteristic_length,
        air_thermal_conductivity,
        peak_temp,
        h0,
        max_iter_num,
    } = io::read_config(CONFIG_PATH).unwrap();

    //tmp
    let (temp_column_num, thermocouple_pos) = match interp_method {
        preprocess::InterpMethod::Horizontal => {
            let mut x = temp_column_num;
            let mut y = thermocouple_pos;
            x.truncate(4);
            y.truncate(4);
            (x, y)
        }
        _ => (temp_column_num, thermocouple_pos),
    };

    println!("read video...");
    let video_record = (start_frame, frame_num, video_path);
    let region_record = (upper_left_pos, region_shape);
    let (g2d, frame_rate) = io::read_video(video_record, region_record).unwrap();
    let dt = 1. / frame_rate as f64;
    let t1 = Instant::now();
    println!("{:?}", t1.duration_since(t0));

    println!("read excel...");
    let temp_record = (start_line, frame_num, &temp_column_num, &excel_path);
    let t2d = io::read_temp_excel(temp_record).unwrap();
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

    println!("start calculating...");
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
}
