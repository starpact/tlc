#[cfg(test)]

pub mod calculate {
    use ndarray::prelude::*;

    use tlc::calculate::*;

    const CONFIG_PATH: &str = "./config/config_large.json";

    fn example_g2d() -> (Array2<u8>, usize) {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            start_frame,
            frame_num,
            video_path,
            upper_left_pos,
            region_shape,
            ..
        } = config_paras;

        let video_record = (start_frame, frame_num, &video_path.to_string());
        let region_record = (upper_left_pos, region_shape);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f64> {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            start_line,
            frame_num,
            temp_colunm_num,
            excel_path,
            ..
        } = config_paras;
        let temp_record = (start_line, frame_num, temp_colunm_num, &excel_path);
        io::read_temp_excel(temp_record).unwrap()
    }

    #[test]
    fn test_read_video() {
        let t0 = std::time::Instant::now();
        let (g2d, frame_rate) = example_g2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", frame_rate);
        println!("{}", g2d.row(0));
    }

    #[test]
    fn test_read_temp_excel() {
        let frame_num = io::read_config(CONFIG_PATH).unwrap().frame_num;
        let t0 = std::time::Instant::now();
        let res = example_t2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(frame_num - 1));
    }

    #[test]
    fn test_detect_peak() {
        let g2d = example_g2d().0;

        let t0 = std::time::Instant::now();
        let peak = preprocess::detect_peak(g2d);
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", peak.slice(s![180000..180100]));
    }

    #[test]
    fn test_interp_x() {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let t2d = example_t2d();
        let tc_x: Vec<i32> = config_paras
            .thermocouple_pos
            .iter()
            .map(|&pos| pos.1 as i32 - config_paras.upper_left_pos.1 as i32)
            .collect();
        let region_shape = config_paras.region_shape;

        let t0 = std::time::Instant::now();
        let interp_x_t2d = preprocess::interp_x(t2d.view(), &tc_x, region_shape.1);
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", t2d.slice(s![..3, ..]));
        println!("=================");
        println!("{}", interp_x_t2d.slice(s![..3, ..]));
    }

    #[test]
    fn test_cal_delta_temps() {
        let t2d = example_t2d();
        let delta_temps = solve::cal_delta_temps(t2d);

        println!("{}", delta_temps.slice(s![..3, ..]));
        println!("{}", delta_temps.sum_axis(Axis(0)));
    }

    #[test]
    fn test_solve() {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            upper_left_pos,
            h0,
            interp_method,
            max_iter_num,
            thermocouple_pos,
            region_shape,
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            peak_temp,
            ..
        } = config_paras;

        let interp_method = match interp_method.as_str() {
            "horizontal" => solve::InterpMethod::Horizontal,
            "vertical" => solve::InterpMethod::Vertical,
            "2d" => solve::InterpMethod::TwoDimension,
            _ => panic!("wrong interpl method, please choose among horizontal/vertical/2d")
        };
        let t0 = std::time::Instant::now();
        println!("read video...");
        let (g2d, frame_rate) = example_g2d();
        let dt = 1. / frame_rate as f64;

        println!("read excel...");
        let t2d = example_t2d();
        println!("detect peak...");
        let peak_frames = preprocess::detect_peak(g2d);

        let tc_x: Vec<i32> = thermocouple_pos
            .iter()
            .map(|&pos| pos.1 as i32 - upper_left_pos.1 as i32)
            .collect();
        println!("interpolate...");
        let interp_x_t2d = preprocess::interp_x(t2d.view(), &tc_x, region_shape.1);

        let const_vals = (
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            dt,
            peak_temp,
        );

        println!("start calculating...");
        let hs = solve::solve(
            const_vals,
            peak_frames,
            interp_x_t2d,
            interp_method,
            h0,
            max_iter_num,
        );
        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", hs.slice(s![..10]));
        let res = hs.iter().fold((0, 0.), |(count, sum), &h| {
            if h.is_finite() {
                (count + 1, sum + h)
            } else {
                (count, sum)
            }
        });
        println!("{}", res.1 / res.0 as f64 * 0.03429 / 0.0276);
    }

    #[test]
    fn test_read_config() {
        let c = io::read_config("./config/config_small.json").unwrap();
        println!("{:#?}", c);
    }
}
