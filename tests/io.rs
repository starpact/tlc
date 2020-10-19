#[cfg(test)]

mod io {
    use ffmpeg_next as ffmpeg;

    use tlc::calculate::io;

    const VIDEO_PATH: &str = "./resource/ed2_50000_1.avi";
    const EXCEL_PATH: &str = "./resource/ed2_50000_1.xlsx";
    const START_FRAME: usize = 0;
    const FRAME_NUM: usize = 1486;
    const UPPER_LEFT_COORD: (usize, usize) = (38, 34);
    const REGION_SHAPE: (usize, usize) = (500, 700);

    #[test]
    fn test_read_video() {
        let video_record = (START_FRAME, FRAME_NUM, &VIDEO_PATH.to_string());
        let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);

        let t0 = std::time::Instant::now();

        let (green_history, frame_rate) = match io::read_video(video_record, region_record) {
            Ok(res) => res,
            Err(ffmpeg::Error::InvalidData) => panic!("please check your frame settings"),
            Err(err) => panic!("{}", err),
        };

        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", frame_rate);
        println!("{}", green_history.row(0));
    }

    #[test]
    fn test_read_temp_excel() {
        let temp_record = (
            START_FRAME,
            FRAME_NUM,
            &vec![1, 3, 4, 6, 7, 9, 11, 12],
            &EXCEL_PATH.to_string(),
        );

        let t0 = std::time::Instant::now();

        let res = io::read_temp_excel(temp_record).unwrap();

        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", res);
    }
}
