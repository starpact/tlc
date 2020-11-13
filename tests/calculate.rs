#[cfg(test)]
#[macro_use]
extern crate lazy_static;

mod calculate {
    use ndarray::prelude::*;

    use tlc::calculate::*;

    const CONFIG_PATH: &str = "./config/config_large.json";

    lazy_static! {
        static ref CONFIG_PARAS: io::ConfigParas = io::read_config(CONFIG_PATH).unwrap();
        static ref VIDEO_PATH: &'static str = CONFIG_PARAS.video_path.as_str();
        static ref EXCEL_PATH: &'static str = CONFIG_PARAS.excel_path.as_str();
        static ref START_FRAME: usize = CONFIG_PARAS.start_frame;
        static ref START_LINE: usize = CONFIG_PARAS.start_line;
        static ref FRAME_NUM: usize = CONFIG_PARAS.frame_num;
        static ref UPPER_LEFT_POS: (usize, usize) = CONFIG_PARAS.upper_left_pos;
        static ref REGION_SHAPE: (usize, usize) = CONFIG_PARAS.region_shape;
        static ref TEMP_COLUMN_NUM: &'static Vec<usize> = &CONFIG_PARAS.temp_column_num;
        static ref THERMOCOUPLE_POS: &'static Vec<(i32, i32)> = &CONFIG_PARAS.thermocouple_pos;
        static ref INTERP_METHOD: preprocess::InterpMethod = CONFIG_PARAS.interp_method;
        static ref FILTER_METHOD: preprocess::FilterMethod = CONFIG_PARAS.filter_method;
        static ref PEAK_TEMP: f64 = CONFIG_PARAS.peak_temp;
        static ref SOLID_THERMAL_CONDUCTIVITY: f64 = CONFIG_PARAS.solid_thermal_conductivity;
        static ref SOLID_THERMAL_DIFFUSIVITY: f64 = CONFIG_PARAS.solid_thermal_diffusivity;
        static ref H0: f64 = CONFIG_PARAS.h0;
        static ref MAX_ITER_NUM: usize = CONFIG_PARAS.max_iter_num;
    }

    #[test]
    fn show_config() {
        let c = io::read_config(CONFIG_PATH).unwrap();
        println!("{:#?}", c);
    }

    fn example_g2d() -> (Array2<u8>, usize) {
        let video_record = (*START_FRAME, *FRAME_NUM, *VIDEO_PATH);
        let region_record = (*UPPER_LEFT_POS, *REGION_SHAPE);
        io::read_video(video_record, region_record).unwrap()
    }

    fn example_t2d() -> Array2<f64> {
        let temp_record = (*START_LINE, *FRAME_NUM, *TEMP_COLUMN_NUM, *EXCEL_PATH);
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

        println!("{}", res.slice(s![..3, ..]));
        println!("{}", res.row(*FRAME_NUM - 1));
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
    fn test_cal_delta_temps() {
        let t2d = example_t2d();
        let delta_temps = solve::cal_delta_temps(t2d);

        println!("{}", delta_temps.slice(s![..3, ..]));
        println!("{}", delta_temps.sum_axis(Axis(0)));
    }

    #[test]
    fn test_solve() {
        let t0 = std::time::Instant::now();

        println!("read video...");
        let (g2d, frame_rate) = example_g2d();
        let dt = 1. / frame_rate as f64;

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
        let const_vals = (
            *SOLID_THERMAL_CONDUCTIVITY,
            *SOLID_THERMAL_DIFFUSIVITY,
            dt,
            *PEAK_TEMP,
        );

        println!("start calculating...");
        let hs = solve::solve(
            const_vals,
            peak_frames,
            interp_temps,
            query_index,
            *H0,
            *MAX_ITER_NUM,
        );
        println!("\ntotal time cost: {:?}\n", std::time::Instant::now().duration_since(t0));
        println!("{}\n", hs.slice(s![..10]));
        let res = hs.iter().fold((0, 0.), |(count, sum), &h| {
            if h.is_finite() {
                (count + 1, sum + h)
            } else {
                (count, sum)
            }
        });
        println!("overall Nu: {}", res.1 / res.0 as f64 * 0.03429 / 0.0276);
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
        let column_num: usize = 15000;
        for g in g2d.column(column_num) {
            raw.push(*g as usize);
        }
        println!("start filtering");
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
}
