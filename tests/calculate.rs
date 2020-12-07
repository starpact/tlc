#[cfg(test)]
#[macro_use]
extern crate lazy_static;

mod calculate {
    use ndarray::prelude::*;

    use tlc::calculate::*;

    const CONFIG_PATH: &str = "./config/config.json";

    lazy_static! {
        static ref CONFIG_PARAS: io::ConfigParas = io::read_config(CONFIG_PATH).unwrap();
        static ref VIDEO_PATH: &'static str = CONFIG_PARAS.video_path.as_str();
        static ref DAQ_PATH: &'static str = CONFIG_PARAS.daq_path.as_str();
        static ref START_FRAME: usize = CONFIG_PARAS.start_frame;
        static ref START_ROW: usize = CONFIG_PARAS.start_row;
        static ref METADATA: (usize, usize, usize, usize) =
            io::get_metadata(*VIDEO_PATH, *DAQ_PATH, *START_FRAME, *START_ROW).unwrap();
        static ref FRAME_NUM: usize = METADATA.0;
        static ref FRAME_RATE: usize = METADATA.1;
        static ref UPPER_LEFT_POS: (usize, usize) = CONFIG_PARAS.upper_left_pos;
        static ref REGION_SHAPE: (usize, usize) = CONFIG_PARAS.region_shape;
        static ref TEMP_COLUMN_NUM: &'static Vec<usize> = &CONFIG_PARAS.temp_column_num;
        static ref THERMOCOUPLE_POS: &'static Vec<(i32, i32)> = &CONFIG_PARAS.thermocouple_pos;
        static ref INTERP_METHOD: preprocess::InterpMethod = CONFIG_PARAS.interp_method;
        static ref FILTER_METHOD: preprocess::FilterMethod = CONFIG_PARAS.filter_method;
        static ref PEAK_TEMP: f64 = CONFIG_PARAS.peak_temp;
        static ref SOLID_THERMAL_CONDUCTIVITY: f64 = CONFIG_PARAS.solid_thermal_conductivity;
        static ref SOLID_THERMAL_DIFFUSIVITY: f64 = CONFIG_PARAS.solid_thermal_diffusivity;
        static ref CHARACTERISTIC_LENGTH: f64 = CONFIG_PARAS.characteristic_length;
        static ref AIR_THERMAL_CONDUCTIVITY: f64 = CONFIG_PARAS.air_thermal_conductivity;
        static ref H0: f64 = CONFIG_PARAS.h0;
        static ref MAX_ITER_NUM: usize = CONFIG_PARAS.max_iter_num;
    }

    #[test]
    fn aaa() {
        println!("{:?}", *METADATA);
    }

    #[test]
    fn show_config() {
        let c = io::read_config(CONFIG_PATH).unwrap();
        println!("{:#?}", c);
    }

    fn example_g2d() -> Array2<u8> {
        let video_record = (*START_FRAME, *FRAME_NUM, *VIDEO_PATH);
        let region_record = (*UPPER_LEFT_POS, *REGION_SHAPE);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f64> {
        let temp_record = (*START_ROW, *FRAME_NUM, *TEMP_COLUMN_NUM, *DAQ_PATH);
        io::read_daq(temp_record).unwrap()
    }

    #[test]
    fn test_read_video() {
        let t0 = std::time::Instant::now();
        let g2d = example_g2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        let row = g2d.row(0);
        println!("{}", row.slice(s![..100]));
    }

    #[test]
    fn test_read_daq() {
        let t0 = std::time::Instant::now();
        let res = example_t2d();
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(*FRAME_NUM - 1));
    }

    #[test]
    fn test_detect_peak() {
        let g2d = example_g2d();

        let t0 = std::time::Instant::now();
        let peak = preprocess::detect_peak(g2d);
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", peak.slice(s![180000..180100]));
    }

    #[test]
    fn test_interp_x() {
        let t2d = example_t2d();

        let t0 = std::time::Instant::now();
        let interp_x_t2d = preprocess::interp(
            t2d.view(),
            *THERMOCOUPLE_POS,
            *INTERP_METHOD,
            *UPPER_LEFT_POS,
            *REGION_SHAPE,
        )
        .0;
        println!("{:?}", std::time::Instant::now().duration_since(t0));

        println!("{}", t2d.slice(s![..3, ..]));
        println!("=================");
        println!("{}", interp_x_t2d.slice(s![..3, ..]));
    }

    #[test]
    fn test_solve() {
        let t0 = std::time::Instant::now();

        println!("read video...");
        let g2d = example_g2d();
        let dt = 1. / *FRAME_RATE as f64;

        println!("read excel...");
        let t2d = example_t2d();

        println!("filtering");
        let g2d_filtered = preprocess::filtering(g2d, *FILTER_METHOD);

        println!("detect peak...");
        let peak_frames = preprocess::detect_peak(g2d_filtered);

        println!("interpolate...");
        let (interp_temps, query_index) = preprocess::interp(
            t2d.view(),
            *THERMOCOUPLE_POS,
            *INTERP_METHOD,
            *UPPER_LEFT_POS,
            *REGION_SHAPE,
        );

        println!("start calculating...");
        let nus = solve::solve(
            *SOLID_THERMAL_CONDUCTIVITY,
            *SOLID_THERMAL_DIFFUSIVITY,
            *CHARACTERISTIC_LENGTH,
            *AIR_THERMAL_CONDUCTIVITY,
            dt,
            *PEAK_TEMP,
            peak_frames,
            interp_temps,
            query_index,
            *H0,
            *MAX_ITER_NUM,
        );

        println!(
            "\ntotal time cost: {:?}\n",
            std::time::Instant::now().duration_since(t0)
        );
        println!("{}\n", nus.slice(s![..6]));
        let (valid_count, valid_sum) = nus.iter().fold((0, 0.), |(count, sum), &h| {
            if h.is_finite() {
                (count + 1, sum + h)
            } else {
                (count, sum)
            }
        });
        println!("overall average Nu: {}", valid_sum / valid_count as f64);
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

        let g2d = example_g2d();
        let column_num: usize = 15000;
        for g in g2d.column(column_num) {
            raw.push(*g as usize);
        }
        let filtered_g2d = preprocess::filtering(g2d, preprocess::FilterMethod::Median(20));
        for g in filtered_g2d.column(column_num) {
            filtered.push(*g as usize);
        }

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

    #[test]
    fn test_contour() -> Result<(), Box<dyn std::error::Error>> {
        let (height, width) = (1000, 1000);
        let root = BitMapBackend::new("plotters/2.png", (width, height)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .x_label_area_size(10)
            .y_label_area_size(10)
            .build_cartesian_2d(0..width, 0..height)?;
        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .draw()?;
        let plotting_area = chart.plotting_area();
        for y in 0..height {
            for x in 0..width {
                plotting_area.draw_pixel(
                    (x, y),
                    &RGBColor((x / 8) as u8, (y / 8) as u8, (x / 8) as u8 + (y / 8) as u8),
                )?;
            }
        }
        Ok(())
    }
}
