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
    // const THERMAL_COUPLE_X: &[usize] = &[100, 200, 300, 400, 500, 600];

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

    // #[test]
    // fn test_interp1d() {
    //     let t2d = example_t2d();
    //     preprocess::interp1d(t2d);
    // }

    #[test]
    fn aaa() {
        let t2d = array![[10., 20., 15.], [50., 70., 30.]];
        let cal_w = 10;
        let cal_h = 3;
        let ul_x = 1;
        let tc_x = &[3, 6, 9];
        let segs: Vec<(i32, i32)> = tc_x
            .iter()
            .zip(tc_x.iter().skip(1))
            .map(|(&l, &r)| (l - ul_x, r - ul_x))
            .collect();
        let mut res = Array2::<f64>::zeros((t2d.nrows(), cal_w * cal_h));

        par_azip!((row0 in t2d.axis_iter(Axis(0)), mut row1 in res.axis_iter_mut(Axis(0))) {
            let mut iter = row1.iter_mut();
            for _ in (0..).step_by(cal_w).take(cal_h) {
                let mut curr = 0;
                for j in 0..cal_w as i32 {
                    if j == segs[curr].1 && curr + 1 < segs.len() {
                        curr += 1;
                    }
                    *(iter.next().unwrap()) = (row0[curr] * (segs[curr].1 - j) as f64 + row0[curr + 1] * (j - segs[curr].0) as f64)
                                    / (segs[curr].1 - segs[curr].0) as f64;
                }
            }
        });

        println!("{}", res);
    }
}
