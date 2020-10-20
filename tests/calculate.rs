#[cfg(test)]

pub mod calculate {
    use ndarray::parallel::prelude::*;
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
        println!("{}", res.slice(s![60..70, ..]));
        println!("{}", res.sum_axis(Axis(0)));
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
    fn test_cmath() {
        println!("{}", solve::erf(1.));
        println!("{}", solve::erfc(1.));
    }

    #[test]
    fn test_interp_x() {
        let t2d = example_t2d();
        let tc_x:Vec<usize> = THERMOCOUPLE_X.iter().map(|&x| x - UPPER_LEFT_COORD.0).collect();
        

    }
}
