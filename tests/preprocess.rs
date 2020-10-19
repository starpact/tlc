#[cfg(test)]

mod preprocess {
    use ndarray::prelude::*;

    use tlc::calculate::{io, preprocess as pre};

    const PATH: &str = "./resource/test.avi";
    const START_FRAME: usize = 0;
    const FRAME_NUM: usize = 2000;
    const UPPER_LEFT_COORD: (usize, usize) = (100, 200);
    const REGION_SHAPE: (usize, usize) = (800, 1000);

    #[test]
    fn test_detect_peak() {
        let video_record = (START_FRAME, FRAME_NUM, &PATH.to_string());
        let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);
        let green_history = io::read_video(video_record, region_record).unwrap().0;

        let t0 = std::time::Instant::now();
        let peak = pre::detect_peak(green_history);
        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", peak.slice(s![180000..180100]));
    }
}
