use super::*;

const PATH: &str = "./resource/test.avi";
const START_FRAME: usize = 0;
const FRAME_NUM: usize = 2000;
const UPPER_LEFT_COORD: (usize, usize) = (100, 200);
const REGION_SHAPE: (usize, usize) = (800, 1000);

#[test]
fn test_read_video() {
    let video_record = (START_FRAME, FRAME_NUM, &PATH.to_string());
    let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);

    let t0 = std::time::Instant::now();

    let (green_history, frame_rate) = match read_video(video_record, region_record) {
        Ok(res) => res,
        Err(ffmpeg::Error::InvalidData) => panic!("please check your frame settings"),
        Err(err) => panic!("{}", err),
    };

    println!("{:?}", std::time::Instant::now().duration_since(t0));
    println!("{}", frame_rate);
    println!("{}", green_history.row(0));
}

#[test]
fn test_detect_peak() {
    let video_record = (START_FRAME, FRAME_NUM, &PATH.to_string());
    let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);
    let green_history = read_video(video_record, region_record).unwrap().0;

    let t0 = std::time::Instant::now();
    let peak = detect_peak(green_history);
    println!("{:?}", std::time::Instant::now().duration_since(t0));
    println!("{}", peak.slice(s![180000..180100]));
}

#[test]
fn test_read_excel() {
    let excel_path = &"E:\\research\\CFD\\cfd_result.xlsx".to_owned();
    let temp_record = (1, 5, &vec![2, 3, 4, 5, 6, 7, 8], excel_path);
    let res = read_excel_temp(temp_record).unwrap();

    println!("{}", res);
}
