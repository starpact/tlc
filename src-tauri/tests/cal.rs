#![allow(dead_code)]
#[cfg(test)]
mod cal {
    use std::time::{Duration, Instant};

    use plotters::prelude::*;

    use tlc::cal::*;

    const CONFIG_PATH: &str = "./cache/default_config.json";

    type Res = Result<(), Box<dyn std::error::Error>>;

    fn init() -> TLCData {
        TLCData::from_path(CONFIG_PATH).unwrap()
    }

    #[test]
    fn show_config() -> Res {
        let tlc_data = init();
        let t = Instant::now();
        println!("{:#?}", tlc_data.get_config());
        println!("{:?}", t.elapsed());

        Ok(())
    }

    #[test]
    fn read_video() -> Res {
        let mut tlc_data = init();
        let t = Instant::now();

        tlc_data.read_video()?;

        println!("{:?}", t.elapsed());

        Ok(())
    }

    #[test]
    fn init_decoder_tool() {
        let mut tlc_data = init();
        let t = Instant::now();

        tlc_data.get_frame(0).unwrap();

        println!("{:?}", t.elapsed());
        std::thread::sleep(Duration::from_secs(1000));
    }

    #[test]
    fn read_daq() -> Res {
        let mut tlc_data = init();
        let t = Instant::now();
        tlc_data.read_daq()?;
        println!("{:?}", t.elapsed());
        println!("{:?}", tlc_data.get_daq().unwrap());

        Ok(())
    }

    #[test]
    fn init_t2d() -> Res {
        let mut tlc_data = init();
        let t = Instant::now();
        tlc_data.init_t2d()?;
        println!("{:?}", t.elapsed());
        println!("{:?}", tlc_data.get_t2d().unwrap());

        Ok(())
    }

    #[test]
    fn detect_peak() -> Res {
        let mut tlc_data = init();
        tlc_data.read_video()?;
        let t = Instant::now();
        tlc_data.detect_peak()?;
        println!("{:?}", t.elapsed());
        println!("{:?}", &tlc_data.get_peak_frames().unwrap()[180000..180100]);

        Ok(())
    }

    #[test]
    fn test_filtering() -> Res {
        let mut tlc_data = init();

        let t0 = Instant::now();
        tlc_data.read_video()?;
        println!("{:?}", t0.elapsed());

        let t0 = Instant::now();
        tlc_data.filtering()?;
        println!("{:?}", t0.elapsed());

        let mut raw = Vec::new();
        let mut filtered = Vec::new();
        let g2d = tlc_data.get_raw_g2d()?;
        let column_num: usize = 180000;
        for g in g2d.column(column_num) {
            raw.push(*g as usize);
        }

        for g in g2d.column(column_num) {
            filtered.push(*g as usize);
        }

        let root = BitMapBackend::new("./cache/1.png", (2400, 800)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root).build_cartesian_2d(0..g2d.nrows(), 0usize..50)?;
        chart.draw_series(LineSeries::new(raw.into_iter().enumerate(), &RED))?;
        chart.draw_series(LineSeries::new(filtered.into_iter().enumerate(), &BLUE))?;

        chart.configure_series_labels().draw()?;

        Ok(())
    }
}