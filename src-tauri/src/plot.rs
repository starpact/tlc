use std::{lazy::SyncOnceCell, path::Path};

use anyhow::Result;
use ndarray::prelude::*;
use plotters::prelude::*;

pub fn draw_area<P: AsRef<Path>>(
    plot_path: P,
    area: ArrayView2<f64>,
    edge_truncation: (f64, f64),
) -> Result<()> {
    let (h, w) = area.dim();
    let root = BitMapBackend::new(&plot_path, (w as u32, h as u32)).into_drawing_area();
    let chart = ChartBuilder::on(&root).build_cartesian_2d(0..w, 0..h)?;
    let pix_plotter = chart.plotting_area();

    let (vmax, vmin) = edge_truncation;
    let delta = vmax - vmin;
    let jet = CELL.get_or_init(|| JET.map(|[r, g, b]| [r * 255., g * 255., b * 255.]));

    let mut iter = area.into_iter();
    for y in 0..h {
        for x in 0..w {
            if let Some(nu) = iter.next() {
                if nu.is_nan() {
                    pix_plotter.draw_pixel((x, y), &WHITE)?;
                    continue;
                }
                let color_index = ((nu.max(vmin).min(vmax) - vmin) / delta * 255.) as usize;
                let [r, g, b] = jet[color_index];
                pix_plotter.draw_pixel((x, y), &RGBColor(r as u8, g as u8, b as u8))?;
            }
        }
    }

    Ok(())
}

static CELL: SyncOnceCell<[[f64; 3]; 256]> = SyncOnceCell::new();

/// jet colormap from Matlab
const JET: [[f64; 3]; 256] = [
    [0., 0., 0.515625000000000],
    [0., 0., 0.531250000000000],
    [0., 0., 0.546875000000000],
    [0., 0., 0.562500000000000],
    [0., 0., 0.578125000000000],
    [0., 0., 0.593750000000000],
    [0., 0., 0.609375000000000],
    [0., 0., 0.625000000000000],
    [0., 0., 0.640625000000000],
    [0., 0., 0.656250000000000],
    [0., 0., 0.671875000000000],
    [0., 0., 0.687500000000000],
    [0., 0., 0.703125000000000],
    [0., 0., 0.718750000000000],
    [0., 0., 0.734375000000000],
    [0., 0., 0.750000000000000],
    [0., 0., 0.765625000000000],
    [0., 0., 0.781250000000000],
    [0., 0., 0.796875000000000],
    [0., 0., 0.812500000000000],
    [0., 0., 0.828125000000000],
    [0., 0., 0.843750000000000],
    [0., 0., 0.859375000000000],
    [0., 0., 0.875000000000000],
    [0., 0., 0.890625000000000],
    [0., 0., 0.906250000000000],
    [0., 0., 0.921875000000000],
    [0., 0., 0.937500000000000],
    [0., 0., 0.953125000000000],
    [0., 0., 0.968750000000000],
    [0., 0., 0.984375000000000],
    [0., 0., 1.],
    [0., 0.0156250000000000, 1.],
    [0., 0.0312500000000000, 1.],
    [0., 0.0468750000000000, 1.],
    [0., 0.0625000000000000, 1.],
    [0., 0.0781250000000000, 1.],
    [0., 0.0937500000000000, 1.],
    [0., 0.109375000000000, 1.],
    [0., 0.125000000000000, 1.],
    [0., 0.140625000000000, 1.],
    [0., 0.156250000000000, 1.],
    [0., 0.171875000000000, 1.],
    [0., 0.187500000000000, 1.],
    [0., 0.203125000000000, 1.],
    [0., 0.218750000000000, 1.],
    [0., 0.234375000000000, 1.],
    [0., 0.250000000000000, 1.],
    [0., 0.265625000000000, 1.],
    [0., 0.281250000000000, 1.],
    [0., 0.296875000000000, 1.],
    [0., 0.312500000000000, 1.],
    [0., 0.328125000000000, 1.],
    [0., 0.343750000000000, 1.],
    [0., 0.359375000000000, 1.],
    [0., 0.375000000000000, 1.],
    [0., 0.390625000000000, 1.],
    [0., 0.406250000000000, 1.],
    [0., 0.421875000000000, 1.],
    [0., 0.437500000000000, 1.],
    [0., 0.453125000000000, 1.],
    [0., 0.468750000000000, 1.],
    [0., 0.484375000000000, 1.],
    [0., 0.500000000000000, 1.],
    [0., 0.515625000000000, 1.],
    [0., 0.531250000000000, 1.],
    [0., 0.546875000000000, 1.],
    [0., 0.562500000000000, 1.],
    [0., 0.578125000000000, 1.],
    [0., 0.593750000000000, 1.],
    [0., 0.609375000000000, 1.],
    [0., 0.625000000000000, 1.],
    [0., 0.640625000000000, 1.],
    [0., 0.656250000000000, 1.],
    [0., 0.671875000000000, 1.],
    [0., 0.687500000000000, 1.],
    [0., 0.703125000000000, 1.],
    [0., 0.718750000000000, 1.],
    [0., 0.734375000000000, 1.],
    [0., 0.750000000000000, 1.],
    [0., 0.765625000000000, 1.],
    [0., 0.781250000000000, 1.],
    [0., 0.796875000000000, 1.],
    [0., 0.812500000000000, 1.],
    [0., 0.828125000000000, 1.],
    [0., 0.843750000000000, 1.],
    [0., 0.859375000000000, 1.],
    [0., 0.875000000000000, 1.],
    [0., 0.890625000000000, 1.],
    [0., 0.906250000000000, 1.],
    [0., 0.921875000000000, 1.],
    [0., 0.937500000000000, 1.],
    [0., 0.953125000000000, 1.],
    [0., 0.968750000000000, 1.],
    [0., 0.984375000000000, 1.],
    [0., 1., 1.],
    [0.0156250000000000, 1., 0.984375000000000],
    [0.0312500000000000, 1., 0.968750000000000],
    [0.0468750000000000, 1., 0.953125000000000],
    [0.0625000000000000, 1., 0.937500000000000],
    [0.0781250000000000, 1., 0.921875000000000],
    [0.0937500000000000, 1., 0.906250000000000],
    [0.109375000000000, 1., 0.890625000000000],
    [0.125000000000000, 1., 0.875000000000000],
    [0.140625000000000, 1., 0.859375000000000],
    [0.156250000000000, 1., 0.843750000000000],
    [0.171875000000000, 1., 0.828125000000000],
    [0.187500000000000, 1., 0.812500000000000],
    [0.203125000000000, 1., 0.796875000000000],
    [0.218750000000000, 1., 0.781250000000000],
    [0.234375000000000, 1., 0.765625000000000],
    [0.250000000000000, 1., 0.750000000000000],
    [0.265625000000000, 1., 0.734375000000000],
    [0.281250000000000, 1., 0.718750000000000],
    [0.296875000000000, 1., 0.703125000000000],
    [0.312500000000000, 1., 0.687500000000000],
    [0.328125000000000, 1., 0.671875000000000],
    [0.343750000000000, 1., 0.656250000000000],
    [0.359375000000000, 1., 0.640625000000000],
    [0.375000000000000, 1., 0.625000000000000],
    [0.390625000000000, 1., 0.609375000000000],
    [0.406250000000000, 1., 0.593750000000000],
    [0.421875000000000, 1., 0.578125000000000],
    [0.437500000000000, 1., 0.562500000000000],
    [0.453125000000000, 1., 0.546875000000000],
    [0.468750000000000, 1., 0.531250000000000],
    [0.484375000000000, 1., 0.515625000000000],
    [0.500000000000000, 1., 0.500000000000000],
    [0.515625000000000, 1., 0.484375000000000],
    [0.531250000000000, 1., 0.468750000000000],
    [0.546875000000000, 1., 0.453125000000000],
    [0.562500000000000, 1., 0.437500000000000],
    [0.578125000000000, 1., 0.421875000000000],
    [0.593750000000000, 1., 0.406250000000000],
    [0.609375000000000, 1., 0.390625000000000],
    [0.625000000000000, 1., 0.375000000000000],
    [0.640625000000000, 1., 0.359375000000000],
    [0.656250000000000, 1., 0.343750000000000],
    [0.671875000000000, 1., 0.328125000000000],
    [0.687500000000000, 1., 0.312500000000000],
    [0.703125000000000, 1., 0.296875000000000],
    [0.718750000000000, 1., 0.281250000000000],
    [0.734375000000000, 1., 0.265625000000000],
    [0.750000000000000, 1., 0.250000000000000],
    [0.765625000000000, 1., 0.234375000000000],
    [0.781250000000000, 1., 0.218750000000000],
    [0.796875000000000, 1., 0.203125000000000],
    [0.812500000000000, 1., 0.187500000000000],
    [0.828125000000000, 1., 0.171875000000000],
    [0.843750000000000, 1., 0.156250000000000],
    [0.859375000000000, 1., 0.140625000000000],
    [0.875000000000000, 1., 0.125000000000000],
    [0.890625000000000, 1., 0.109375000000000],
    [0.906250000000000, 1., 0.0937500000000000],
    [0.921875000000000, 1., 0.0781250000000000],
    [0.937500000000000, 1., 0.0625000000000000],
    [0.953125000000000, 1., 0.0468750000000000],
    [0.968750000000000, 1., 0.0312500000000000],
    [0.984375000000000, 1., 0.0156250000000000],
    [1., 1., 0.],
    [1., 0.984375000000000, 0.],
    [1., 0.968750000000000, 0.],
    [1., 0.953125000000000, 0.],
    [1., 0.937500000000000, 0.],
    [1., 0.921875000000000, 0.],
    [1., 0.906250000000000, 0.],
    [1., 0.890625000000000, 0.],
    [1., 0.875000000000000, 0.],
    [1., 0.859375000000000, 0.],
    [1., 0.843750000000000, 0.],
    [1., 0.828125000000000, 0.],
    [1., 0.812500000000000, 0.],
    [1., 0.796875000000000, 0.],
    [1., 0.781250000000000, 0.],
    [1., 0.765625000000000, 0.],
    [1., 0.750000000000000, 0.],
    [1., 0.734375000000000, 0.],
    [1., 0.718750000000000, 0.],
    [1., 0.703125000000000, 0.],
    [1., 0.687500000000000, 0.],
    [1., 0.671875000000000, 0.],
    [1., 0.656250000000000, 0.],
    [1., 0.640625000000000, 0.],
    [1., 0.625000000000000, 0.],
    [1., 0.609375000000000, 0.],
    [1., 0.593750000000000, 0.],
    [1., 0.578125000000000, 0.],
    [1., 0.562500000000000, 0.],
    [1., 0.546875000000000, 0.],
    [1., 0.531250000000000, 0.],
    [1., 0.515625000000000, 0.],
    [1., 0.500000000000000, 0.],
    [1., 0.484375000000000, 0.],
    [1., 0.468750000000000, 0.],
    [1., 0.453125000000000, 0.],
    [1., 0.437500000000000, 0.],
    [1., 0.421875000000000, 0.],
    [1., 0.406250000000000, 0.],
    [1., 0.390625000000000, 0.],
    [1., 0.375000000000000, 0.],
    [1., 0.359375000000000, 0.],
    [1., 0.343750000000000, 0.],
    [1., 0.328125000000000, 0.],
    [1., 0.312500000000000, 0.],
    [1., 0.296875000000000, 0.],
    [1., 0.281250000000000, 0.],
    [1., 0.265625000000000, 0.],
    [1., 0.250000000000000, 0.],
    [1., 0.234375000000000, 0.],
    [1., 0.218750000000000, 0.],
    [1., 0.203125000000000, 0.],
    [1., 0.187500000000000, 0.],
    [1., 0.171875000000000, 0.],
    [1., 0.156250000000000, 0.],
    [1., 0.140625000000000, 0.],
    [1., 0.125000000000000, 0.],
    [1., 0.109375000000000, 0.],
    [1., 0.0937500000000000, 0.],
    [1., 0.0781250000000000, 0.],
    [1., 0.0625000000000000, 0.],
    [1., 0.0468750000000000, 0.],
    [1., 0.0312500000000000, 0.],
    [1., 0.0156250000000000, 0.],
    [1., 0., 0.],
    [0.984375000000000, 0., 0.],
    [0.968750000000000, 0., 0.],
    [0.953125000000000, 0., 0.],
    [0.937500000000000, 0., 0.],
    [0.921875000000000, 0., 0.],
    [0.906250000000000, 0., 0.],
    [0.890625000000000, 0., 0.],
    [0.875000000000000, 0., 0.],
    [0.859375000000000, 0., 0.],
    [0.843750000000000, 0., 0.],
    [0.828125000000000, 0., 0.],
    [0.812500000000000, 0., 0.],
    [0.796875000000000, 0., 0.],
    [0.781250000000000, 0., 0.],
    [0.765625000000000, 0., 0.],
    [0.750000000000000, 0., 0.],
    [0.734375000000000, 0., 0.],
    [0.718750000000000, 0., 0.],
    [0.703125000000000, 0., 0.],
    [0.687500000000000, 0., 0.],
    [0.671875000000000, 0., 0.],
    [0.656250000000000, 0., 0.],
    [0.640625000000000, 0., 0.],
    [0.625000000000000, 0., 0.],
    [0.609375000000000, 0., 0.],
    [0.593750000000000, 0., 0.],
    [0.578125000000000, 0., 0.],
    [0.562500000000000, 0., 0.],
    [0.546875000000000, 0., 0.],
    [0.531250000000000, 0., 0.],
    [0.515625000000000, 0., 0.],
    [0.500000000000000, 0., 0.],
];
