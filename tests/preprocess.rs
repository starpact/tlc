#[cfg(test)]

mod preprocess {
    use ndarray::prelude::*;

    use tlc::calculate::{io, preprocess as pre};

    const VIDEO_PATH: &str = "./resource/ed2_50000_1.avi";
    const START_FRAME: usize = 80;
    const FRAME_NUM: usize = 1486;
    const UPPER_LEFT_COORD: (usize, usize) = (38, 34);
    const REGION_SHAPE: (usize, usize) = (500, 700);

    #[test]
    fn test_detect_peak() {
        let video_record = (START_FRAME, FRAME_NUM, &VIDEO_PATH.to_string());
        let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);
        let g2d = io::read_video(video_record, region_record).unwrap().0;

        let t0 = std::time::Instant::now();
        let peak = pre::detect_peak(g2d);
        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", peak.slice(s![180000..180100]));
    }
}
