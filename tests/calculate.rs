#[cfg(test)]
mod calculate {
    use std::time::Instant;

    use ndarray::prelude::*;
    use once_cell::sync::Lazy;

    use tlc::calculate::*;

    const CONFIG_PATH: &str = "./config/config.json";

    static CONFIG_PARAS: Lazy<io::ConfigParas> =
        Lazy::new(|| io::read_config(CONFIG_PATH).unwrap());
    static VIDEO_PATH: Lazy<&'static str> = Lazy::new(|| CONFIG_PARAS.video_path.as_str());
    static DAQ_PATH: Lazy<&'static str> = Lazy::new(|| CONFIG_PARAS.daq_path.as_str());
    static SAVE_DIR: Lazy<&'static str> = Lazy::new(|| CONFIG_PARAS.save_dir.as_str());
    static START_FRAME: Lazy<usize> = Lazy::new(|| CONFIG_PARAS.start_frame);
    static START_ROW: Lazy<usize> = Lazy::new(|| CONFIG_PARAS.start_row);
    static METADATA: Lazy<(usize, usize, usize, usize)> =
        Lazy::new(|| io::get_metadata(*VIDEO_PATH, *DAQ_PATH, *START_FRAME, *START_ROW).unwrap());
    static FRAME_NUM: Lazy<usize> = Lazy::new(|| METADATA.0);
    static FRAME_RATE: Lazy<usize> = Lazy::new(|| METADATA.1);
    static TOP_LEFT_POS: Lazy<(usize, usize)> = Lazy::new(|| CONFIG_PARAS.top_left_pos);
    static REGION_SHAPE: Lazy<(usize, usize)> = Lazy::new(|| CONFIG_PARAS.region_shape);
    static TEMP_COLUMN_NUM: Lazy<&'static Vec<usize>> = Lazy::new(|| &CONFIG_PARAS.temp_column_num);
    static THERMOCOUPLE_POS: Lazy<&'static Vec<(i32, i32)>> =
        Lazy::new(|| &CONFIG_PARAS.thermocouple_pos);
    static INTERP_METHOD: Lazy<preprocess::InterpMethod> = Lazy::new(|| CONFIG_PARAS.interp_method);
    static FILTER_METHOD: Lazy<preprocess::FilterMethod> = Lazy::new(|| CONFIG_PARAS.filter_method);
    static PEAK_TEMP: Lazy<f32> = Lazy::new(|| CONFIG_PARAS.peak_temp);
    static SOLID_THERMAL_CONDUCTIVITY: Lazy<f32> =
        Lazy::new(|| CONFIG_PARAS.solid_thermal_conductivity);
    static SOLID_THERMAL_DIFFUSIVITY: Lazy<f32> =
        Lazy::new(|| CONFIG_PARAS.solid_thermal_diffusivity);
    static CHARACTERISTIC_LENGTH: Lazy<f32> = Lazy::new(|| CONFIG_PARAS.characteristic_length);
    static AIR_THERMAL_CONDUCTIVITY: Lazy<f32> =
        Lazy::new(|| CONFIG_PARAS.air_thermal_conductivity);
    static H0: Lazy<f32> = Lazy::new(|| CONFIG_PARAS.h0);
    static MAX_ITER_NUM: Lazy<usize> = Lazy::new(|| CONFIG_PARAS.max_iter_num);

    #[test]
    fn show_config() {
        let c = io::read_config(CONFIG_PATH).unwrap();
        println!("{:#?}", c);
    }

    fn example_g2d() -> Array2<u8> {
        let video_record = (*START_FRAME, *FRAME_NUM, *VIDEO_PATH);
        let region_record = (*TOP_LEFT_POS, *REGION_SHAPE);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f32> {
        let temp_record = (*START_ROW, *FRAME_NUM, *TEMP_COLUMN_NUM, *DAQ_PATH);
        io::read_daq(temp_record).unwrap()
    }

    #[test]
    fn test_read_video() {
        let t0 = Instant::now();
        let g2d = example_g2d();
        println!("{:?}", Instant::now().duration_since(t0));

        let row = g2d.row(0);
        println!("{}", row.slice(s![..10]));
    }

    #[test]
    fn test_read_daq() {
        let t0 = Instant::now();
        let res = example_t2d();
        println!("{:?}", Instant::now().duration_since(t0));

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(*FRAME_NUM - 1));
    }

    #[test]
    fn test_detect_peak() {
        let g2d = example_g2d();

        let t0 = Instant::now();
        let peak = preprocess::detect_peak(g2d.view());
        println!("{:?}", Instant::now().duration_since(t0));

        println!("{}", peak.slice(s![180000..180100]));
    }

    #[test]
    fn test_interp_x() {
        let t2d = example_t2d();

        let t0 = Instant::now();
        let interp_x_t2d = preprocess::interp(
            t2d.view(),
            *THERMOCOUPLE_POS,
            *INTERP_METHOD,
            *TOP_LEFT_POS,
            *REGION_SHAPE,
        )
        .0;
        println!("{:?}", Instant::now().duration_since(t0));
        postprocess::simple_plot(interp_x_t2d.row(1000)).unwrap();
    }

    #[test]
    fn test_solve() {
        let t0 = Instant::now();

        println!("read video...");
        let mut g2d = example_g2d();
        let dt = 1. / *FRAME_RATE as f32;

        println!("read excel...");
        let t2d = example_t2d();

        println!("filtering");
        preprocess::filtering(g2d.view_mut(), *FILTER_METHOD);

        println!("detect peak...");
        let peak_frames = preprocess::detect_peak(g2d.view());

        println!("interpolate...");
        let (interp_temps, query_index) = preprocess::interp(
            t2d.view(),
            *THERMOCOUPLE_POS,
            *INTERP_METHOD,
            *TOP_LEFT_POS,
            *REGION_SHAPE,
        );

        println!("start calculating...");
        let nus = solve::solve(
            peak_frames.view(),
            interp_temps.view(),
            query_index.view(),
            *SOLID_THERMAL_CONDUCTIVITY,
            *SOLID_THERMAL_DIFFUSIVITY,
            *CHARACTERISTIC_LENGTH,
            *AIR_THERMAL_CONDUCTIVITY,
            dt,
            *PEAK_TEMP,
            *H0,
            *MAX_ITER_NUM,
        );

        println!(
            "\ntotal time cost: {:?}\n",
            Instant::now().duration_since(t0)
        );
        let (valid_count, valid_sum) = nus.iter().fold((0, 0.), |(count, sum), &nu| {
            if nu.is_finite() {
                (count + 1, sum + nu)
            } else {
                (count, sum)
            }
        });
        println!("overall average Nu: {}", valid_sum / valid_count as f32);
    }

    use plotters::prelude::*;

    #[test]
    fn test_filtering() {
        let mut raw = Vec::new();
        let mut filtered = Vec::new();

        let mut g2d = example_g2d();
        let column_num: usize = 180000;
        for g in g2d.column(column_num) {
            raw.push(*g as usize);
        }

        let t0 = Instant::now();
        preprocess::filtering(g2d.view_mut(), preprocess::FilterMethod::Median(20));
        println!("{:?}", Instant::now().duration_since(t0));

        for g in g2d.column(column_num) {
            filtered.push(*g as usize);
        }

        let root = BitMapBackend::new("plotters/1.png", (2400, 800)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let mut chart = ChartBuilder::on(&root)
            .build_cartesian_2d(0..g2d.nrows(), 0usize..50)
            .unwrap();
        chart
            .draw_series(LineSeries::new(raw.into_iter().enumerate(), &RED))
            .unwrap();
        chart
            .draw_series(LineSeries::new(filtered.into_iter().enumerate(), &BLUE))
            .unwrap();

        chart.configure_series_labels().draw().unwrap();
    }

    #[test]
    fn have_a_look() {
        let (nu_path, mut plot_path) = io::get_save_path(*VIDEO_PATH, *SAVE_DIR).unwrap();
        println!("{:?}", nu_path);
        println!("{:?}", plot_path);
        let nu2d = io::read_nu(&nu_path).unwrap();
        let nu_nan_mean = postprocess::cal_average(nu2d.view()).0;

        let original_stem = plot_path.file_stem().unwrap().to_owned();

        let mut cnt = 1;
        while plot_path.exists() {
            let mut file_stem = original_stem.clone();
            file_stem.push(cnt.to_string() + ".png");
            plot_path.set_file_name(file_stem);
            cnt += 1;
        }
        postprocess::plot_nu(nu2d.view(), nu_nan_mean, plot_path).unwrap();
    }
}
