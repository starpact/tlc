use std::path::Path;

use plotters::prelude::*;

use ndarray::prelude::*;

use super::colormap::JET;
use super::error::TLCResult;
use crate::awsl;

pub fn cal_nan_mean<D: Dimension>(data: ArrayView<f32, D>) -> f32 {
    let (sum, cnt) = data.iter().fold((0., 0), |(s, cnt), &x| {
        if x.is_nan() {
            (s, cnt)
        } else {
            (s + x, cnt + 1)
        }
    });
    let nan_mean = sum / cnt as f32;

    nan_mean
}

pub fn plot_area<P: AsRef<Path>>(
    plot_path: P,
    area: ArrayView2<f32>,
    vmin: f32,
    vmax: f32,
) -> TLCResult<()> {
    let (h, w) = area.dim();
    let root = BitMapBackend::new(&plot_path, (w as u32, h as u32)).into_drawing_area();
    let chart = ChartBuilder::on(&root)
        .build_cartesian_2d(0..w, 0..h)
        .map_err(|err| awsl!(PlotError, err))?;
    let pix_plotter = chart.plotting_area();

    let delta = vmax - vmin;

    let mut it = area.iter();
    for y in 0..h {
        for x in 0..w {
            if let Some(nu) = it.next() {
                if nu.is_nan() {
                    continue;
                }
                let color_index = ((nu.max(vmin).min(vmax) - vmin) / delta * 255.) as usize;
                let mut rgb = JET[color_index];
                rgb.iter_mut().for_each(|c| *c = *c * 255.);
                pix_plotter
                    .draw_pixel((x, y), &RGBColor(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8))
                    .map_err(|err| awsl!(PlotError, err))?;
            }
        }
    }

    Ok(())
}

pub fn plot_line(arr: ArrayView1<f32>) -> TLCResult<()> {
    let len = arr.len();
    let x0 = *arr.first().ok_or(awsl!(PlotError, "empty data"))?;
    let min = arr.iter().fold(x0, |m, &x| if x < m { x } else { m });
    let max = arr.iter().fold(x0, |m, &x| if x > m { x } else { m });
    let delta = max - min;

    let root = BitMapBackend::new("cache/simple_plot.png", (640, 480)).into_drawing_area();
    root.fill(&WHITE).map_err(|err| awsl!(PlotError, err))?;
    let mut chart = ChartBuilder::on(&root)
        .margin(30)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(0..len, (min - delta * 0.1)..(max + delta * 0.1))
        .map_err(|err| awsl!(PlotError, err))?;
    chart
        .configure_mesh()
        .draw()
        .map_err(|err| awsl!(PlotError, err))?;
    chart
        .draw_series(LineSeries::new(
            arr.iter().enumerate().map(|(i, v)| (i, *v)),
            &RED,
        ))
        .map_err(|err| awsl!(PlotError, err))?;

    Ok(())
}
