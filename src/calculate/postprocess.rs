use std::error::Error;
use std::path::Path;

use plotters::prelude::*;

use ndarray::prelude::*;

use super::colormap::JET;

pub fn cal_average<D: Dimension>(nus: ArrayView<f64, D>) -> (f64, f64) {
    let (sum, cnt) = nus.iter().fold((0., 0), |(s, cnt), &nu| {
        if nu.is_nan() {
            (s, cnt)
        } else {
            (s + nu, cnt + 1)
        }
    });
    let nan_cnt = nus.len() - cnt;
    let nan_ratio = nan_cnt as f64 / cnt as f64;
    let nan_mean = sum / cnt as f64;

    (nan_mean, nan_ratio)
}

pub fn plot_nu<P: AsRef<Path>>(
    nu2d: ArrayView2<f64>,
    nan_mean: f64,
    plot_path: P,
) -> Result<(), Box<dyn Error>> {
    let (height, width) = nu2d.dim();
    let root = BitMapBackend::new(&plot_path, (width as u32, height as u32)).into_drawing_area();
    root.fill(&WHITE)?;
    let mut chart = ChartBuilder::on(&root).build_cartesian_2d(0..width, 0..height)?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw()?;
    let pix_plotter = chart.plotting_area();

    let (vmin, vmax) = (nan_mean * 0.4, nan_mean * 2.);
    let delta = vmax - vmin;

    let mut it = nu2d.iter();
    for y in 0..height {
        for x in 0..width {
            let nu = *it.next().ok_or("bakana")?;
            if nu.is_nan() {
                continue;
            }
            let color_index = ((nu.max(vmin).min(vmax) - vmin) / delta * 255.) as usize;
            let rgb: Vec<_> = JET[color_index].iter().map(|c| (c * 255.) as u8).collect();
            pix_plotter.draw_pixel((x, y), &RGBColor(rgb[0], rgb[1], rgb[2]))?;
        }
    }

    Ok(())
}

pub fn plot_temps(temps: ArrayView1<f64>) -> Result<(), Box<dyn Error>> {
    let len = temps.len();
    let mean = temps.mean().ok_or("bakana")?;

    let root = BitMapBackend::new("plotters/test_temps_interp.png", (640, 480)).into_drawing_area();
    root.fill(&WHITE)?;
    let mut chart = ChartBuilder::on(&root).build_cartesian_2d(0..len, mean * 0.7..mean * 1.3)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(
        temps.iter().enumerate().map(|(i, v)| (i, *v)),
        &RED,
    ))?;

    Ok(())
}
