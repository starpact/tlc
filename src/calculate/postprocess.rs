use std::path::Path;

use plotters::prelude::*;

use ndarray::prelude::*;

use super::super::err;
use super::colormap::JET;
use super::error::TLCResult;

pub fn cal_average<D: Dimension>(nus: ArrayView<f32, D>) -> (f32, f32) {
    let (sum, cnt) = nus.iter().fold((0., 0), |(s, cnt), &nu| {
        if nu.is_nan() {
            (s, cnt)
        } else {
            (s + nu, cnt + 1)
        }
    });
    let nan_cnt = nus.len() - cnt;
    let nan_ratio = nan_cnt as f32 / cnt as f32;
    let nan_mean = sum / cnt as f32;

    (nan_mean, nan_ratio)
}

pub fn plot_nu<P: AsRef<Path>>(
    nu2d: ArrayView2<f32>,
    vmin: f32,
    vmax: f32,
    plot_path: P,
) -> TLCResult<()> {
    let (height, width) = nu2d.dim();
    let root = BitMapBackend::new(&plot_path, (width as u32, height as u32)).into_drawing_area();
    root.fill(&WHITE).map_err(|err| err!(PlotError, err))?;
    let mut chart = ChartBuilder::on(&root)
        .build_cartesian_2d(0..width, 0..height)
        .map_err(|err| err!(PlotError, err))?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw()
        .map_err(|err| err!(PlotError, err))?;
    let pix_plotter = chart.plotting_area();

    let delta = vmax - vmin;

    let mut it = nu2d.iter();
    for y in 0..height {
        for x in 0..width {
            if let Some(nu) = it.next() {
                if nu.is_nan() {
                    continue;
                }
                let color_index = ((nu.max(vmin).min(vmax) - vmin) / delta * 255.) as usize;
                let rgb: Vec<_> = JET[color_index].iter().map(|c| (c * 255.) as u8).collect();
                pix_plotter
                    .draw_pixel((x, y), &RGBColor(rgb[0], rgb[1], rgb[2]))
                    .map_err(|err| err!(PlotError, err))?;
            }
        }
    }

    Ok(())
}

pub fn simple_plot(arr: ArrayView1<f32>) -> TLCResult<()> {
    let len = arr.len();
    let x0 = *arr.first().ok_or(err!(PlotError, "empty data"))?;
    let min = arr.into_iter().fold(x0, |m, &x| if x < m { x } else { m });
    let max = arr.into_iter().fold(x0, |m, &x| if x > m { x } else { m });
    let delta = max - min;

    let root = BitMapBackend::new("plotters/simple_plot.png", (640, 480)).into_drawing_area();
    root.fill(&WHITE).map_err(|err| err!(PlotError, err))?;
    let mut chart = ChartBuilder::on(&root)
        .margin(30)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(0..len, (min - delta * 0.1)..(max + delta * 0.1))
        .map_err(|err| err!(PlotError, err))?;
    chart
        .configure_mesh()
        .draw()
        .map_err(|err| err!(PlotError, err))?;
    chart
        .draw_series(LineSeries::new(
            arr.iter().enumerate().map(|(i, v)| (i, *v)),
            &RED,
        ))
        .map_err(|err| err!(PlotError, err))?;

    Ok(())
}
