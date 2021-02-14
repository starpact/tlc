#![allow(dead_code)]
#[cfg(test)]
mod cal {
    use std::time::Instant;

    use tlc::cal::*;

    const CONFIG_PATH: &str = "./tmp/config/config.json";

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
        println!("{}", tlc_data.get_raw_g2d().unwrap());

        Ok(())
    }

    #[test]
    fn read_daq() -> Res {
        let mut tlc_data = init();
        let t = Instant::now();
        tlc_data.read_daq()?;
        println!("{:?}", t.elapsed());
        println!("{}", tlc_data.get_t2d().unwrap());

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
    fn interp() -> Res {
        let mut tlc_data = init();
        tlc_data.read_daq()?;
        let t = Instant::now();
        tlc_data.interp()?;
        println!("{:?}", t.elapsed());
        postprocess::plot_line(tlc_data.interp_single_point(1000).unwrap())?;

        Ok(())
    }

    use plotters::prelude::*;

    #[test]
    fn test_filtering() -> Res {
        let mut tlc_data = init();
        let mut raw = Vec::new();
        let mut filtered = Vec::new();

        tlc_data.read_video()?;
        tlc_data.filtering()?;

        let tlc_data = tlc_data;
        let g2d = tlc_data.get_raw_g2d().unwrap();
        let column_num: usize = 180000;
        for g in g2d.column(column_num) {
            raw.push(*g as usize);
        }

        for g in g2d.column(column_num) {
            filtered.push(*g as usize);
        }

        let root = BitMapBackend::new("./tmp/plots/1.png", (2400, 800)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root).build_cartesian_2d(0..g2d.nrows(), 0usize..50)?;
        chart.draw_series(LineSeries::new(raw.into_iter().enumerate(), &RED))?;
        chart.draw_series(LineSeries::new(filtered.into_iter().enumerate(), &BLUE))?;

        chart.configure_series_labels().draw()?;

        Ok(())
    }
}
