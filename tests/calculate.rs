#[cfg(test)]

pub mod calculate {
    use ndarray::prelude::*;

    use tlc::calculate::*;

    const CONFIG_PATH: &str = "./config/config_large.json";

    fn example_g2d() -> (Array2<u8>, usize) {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            start_frame,
            frame_num,
            video_path,
            upper_left_pos,
            region_shape,
            ..
        } = config_paras;

        let video_record = (start_frame, frame_num, &video_path.to_string());
        let region_record = (upper_left_pos, region_shape);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f64> {
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            start_line,
            frame_num,
            temp_column_num,
            excel_path,
            ..
        } = config_paras;
        let temp_record = (start_line, frame_num, temp_column_num, &excel_path);
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
        let frame_num = io::read_config(CONFIG_PATH).unwrap().frame_num;
        let t0 = std::time::Instant::now();
        let res = example_t2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(frame_num - 1));
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
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let t2d = example_t2d();
        let io::ConfigParas {
            region_shape,
            thermocouple_pos,
            upper_left_pos,
            ..
        } = config_paras;
        let interp_method = match config_paras.interp_method.as_str() {
            "horizontal" => preprocess::InterpMethod::Horizontal,
            "vertical" => preprocess::InterpMethod::Vertical,
            "2d" => preprocess::InterpMethod::TwoDimension,
            _ => panic!("wrong interpl method, please choose among horizontal/vertical/2d"),
        };

        let t0 = std::time::Instant::now();
        let interp_x_t2d = preprocess::interp(
            t2d.view(),
            &thermocouple_pos,
            interp_method,
            upper_left_pos,
            region_shape,
        )
        .0;
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
        let config_paras = io::read_config(CONFIG_PATH).unwrap();
        let io::ConfigParas {
            upper_left_pos,
            h0,
            interp_method,
            max_iter_num,
            thermocouple_pos,
            region_shape,
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            peak_temp,
            ..
        } = config_paras;

        let interp_method = match interp_method.as_str() {
            "horizontal" => preprocess::InterpMethod::Horizontal,
            "vertical" => preprocess::InterpMethod::Vertical,
            "2d" => preprocess::InterpMethod::TwoDimension,
            _ => panic!("wrong interpl method, please choose among horizontal/vertical/2d"),
        };
        let t0 = std::time::Instant::now();
        println!("read video...");
        let (g2d, frame_rate) = example_g2d();
        let dt = 1. / frame_rate as f64;

        println!("read excel...");
        let t2d = example_t2d();
        println!("filtering");
        let filtered_g2d = preprocess::filtering(g2d, preprocess::FilterMethod::Median(20));
        println!("detect peak...");
        let peak_frames = preprocess::detect_peak(filtered_g2d);

        println!("interpolate...");
        let (interp_t2d, query_index) = preprocess::interp(
            t2d.view(),
            &thermocouple_pos,
            interp_method,
            upper_left_pos,
            region_shape,
        );
        let const_vals = (
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            dt,
            peak_temp,
        );

        println!("start calculating...");
        let hs = solve::solve(
            const_vals,
            peak_frames,
            interp_t2d,
            query_index,
            h0,
            max_iter_num,
        );
        println!("{:?}", std::time::Instant::now().duration_since(t0));
        println!("{}", hs.slice(s![..10]));
        let res = hs.iter().fold((0, 0.), |(count, sum), &h| {
            if h.is_finite() {
                (count + 1, sum + h)
            } else {
                (count, sum)
            }
        });
        println!("{}", res.1 / res.0 as f64 * 0.03429 / 0.0276);
    }

    #[test]
    fn test_read_config() {
        let c = io::read_config("./config/config_small.json").unwrap();
        println!("{:#?}", c);
    }

    use plotters::prelude::*;
    #[test]
    fn test_plot() {
        let root = BitMapBackend::new("plotters/0.png", (640, 480)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let mut chart = ChartBuilder::on(&root)
            .caption("y=x^2", ("sans-serif", 50).into_font())
            .margin(5)
            .x_label_area_size(30)
            .y_label_area_size(30)
            .build_cartesian_2d(-1f32..1f32, -0.1f32..1f32)
            .unwrap();
        chart.configure_mesh().draw().unwrap();
        chart
            .draw_series(LineSeries::new(
                (-50..=50).map(|x| x as f32 / 50.).map(|x| (x, x * x)),
                &RED,
            ))
            .unwrap()
            .label("y=x^2")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));
        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();
    }

    #[test]
    fn test_filtering() {
        let mut raw = Vec::new();
        let mut filtered = Vec::new();

        let g2d = example_g2d().0;
        const COLUMN_NUM: usize = 15000;
        for g in g2d.column(COLUMN_NUM) {
            raw.push(*g as usize);
        }
        println!("start filtering");
        let filtered_g2d = preprocess::filtering(g2d, preprocess::FilterMethod::Median(20));
        for g in filtered_g2d.column(COLUMN_NUM) {
            filtered.push(*g as usize);
        }
        // println!("{:?}", raw);
        // println!("{:?}",filtered);

        let root = BitMapBackend::new("plotters/1.png", (2400, 800)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let mut chart = ChartBuilder::on(&root)
            .build_cartesian_2d(0..filtered_g2d.nrows(), 80usize..180)
            .unwrap();
        chart
            .draw_series(LineSeries::new(raw.into_iter().enumerate(), &RED))
            .unwrap();
        chart
            .draw_series(LineSeries::new(filtered.into_iter().enumerate(), &BLUE))
            .unwrap();

        chart.configure_series_labels().draw().unwrap();
    }
}
