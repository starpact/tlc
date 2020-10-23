#[cfg(test)]

pub mod calculate {
    use ndarray::prelude::*;

    use tlc::calculate::*;

    const VIDEO_PATH: &str = "./resource/ed2_50000_1.avi";
    const EXCEL_PATH: &str = "./resource/ed2_50000_1.xlsx";
    const START_FRAME: usize = 80;
    const FRAME_NUM: usize = 1486;
    const UPPER_LEFT_COORD: (usize, usize) = (38, 34);
    const REGION_SHAPE: (usize, usize) = (500, 700);
    const TEMP_COLUMNS: &[usize] = &[1, 3, 4, 6, 7, 9];
    const THERMOCOUPLE_X: &[usize] = &[100, 200, 300, 400, 500, 600];
    const PEAK_TEMPERATURE: f64 = 35.18;
    const SOLID_THERMAL_CONDUCTIVITY: f64 = 0.19;
    const SOLID_THERMAL_DIFFUSIVITY: f64 = 1.091e-7;
    const H0: f64 = 50.;
    const MAX_ITER_NUM: usize = 5;

    fn example_g2d() -> (Array2<u8>, usize) {
        let video_record = (START_FRAME, FRAME_NUM, &VIDEO_PATH.to_string());
        let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f64> {
        let temp_record = (
            START_FRAME,
            FRAME_NUM,
            TEMP_COLUMNS,
            &EXCEL_PATH.to_string(),
        );
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
        let t0 = std::time::Instant::now();
        let res = example_t2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(FRAME_NUM - 1));
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
        let t2d = example_t2d();
        let ul_x = UPPER_LEFT_COORD.0;
        let tc_x: Vec<i32> = THERMOCOUPLE_X.iter().map(|&x| (x - ul_x) as i32).collect();

        let t0 = std::time::Instant::now();
        let interp_x_t2d = preprocess::interp_x(t2d.view(), &tc_x, REGION_SHAPE.1);
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
        let t0 = std::time::Instant::now();
        let (g2d, frame_rate) = example_g2d();
        let dt = 1. / frame_rate as f64;

        let t2d = example_t2d();
        let peak_frames = preprocess::detect_peak(g2d);

        let tc_x: Vec<i32> = THERMOCOUPLE_X
            .iter()
            .map(|&x| (x - UPPER_LEFT_COORD.1) as i32)
            .collect();
        let interp_x_t2d = preprocess::interp_x(t2d.view(), &tc_x, REGION_SHAPE.1);

        let const_vals = (
            SOLID_THERMAL_CONDUCTIVITY,
            SOLID_THERMAL_DIFFUSIVITY,
            dt,
            PEAK_TEMPERATURE,
        );

        println!("start calculating...");
        let hs = solve::solve(
            const_vals,
            peak_frames,
            interp_x_t2d,
            solve::InterpMethod::Horizontal,
            H0,
            MAX_ITER_NUM,
        );
        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", hs.slice(s![..7]));
        let res = hs.iter().fold((0, 0.), |(count, sum), &h| {
            if h.is_finite() {
                (count + 1, sum + h)
            } else {
                (count, sum)
            }
        });
        println!("{}", res.1 / res.0 as f64);
    }
}
