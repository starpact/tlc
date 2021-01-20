use median::Filter;

use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

use serde::{Deserialize, Serialize};

use packed_simd::f32x8;

use super::error::TLCResult;
use crate::err;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum FilterMethod {
    No,
    Median(usize),
}

/// filter the green history of each pixel along time axis
pub fn filtering(mut g2d: ArrayViewMut2<u8>, filter_method: FilterMethod) {
    match filter_method {
        FilterMethod::Median(window_size) => {
            g2d.axis_iter_mut(Axis(1))
                .into_par_iter()
                .for_each(|mut col| {
                    let mut filter = Filter::new(window_size);
                    col.iter_mut().for_each(|g| *g = filter.consume(*g))
                });
        }
        _ => {}
    }
}

/// traverse along the timeline to detect the peak of green values and record that frame index
pub fn detect_peak(g2d: ArrayView2<u8>) -> TLCResult<Vec<usize>> {
    let mut peak_frames = Vec::with_capacity(g2d.ncols());
    unsafe { peak_frames.set_len(g2d.ncols()) };

    g2d.axis_iter(Axis(1))
        .into_par_iter()
        .zip(peak_frames.par_iter_mut())
        .try_for_each(|(col, p)| -> TLCResult<()> {
            *p = col
                .iter()
                .enumerate()
                .max_by_key(|(_, g)| *g)
                .ok_or(err!())?
                .0;

            Ok(())
        })?;

    Ok(peak_frames)
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum InterpMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear((usize, usize)),
    BilinearExtra((usize, usize)),
}

pub struct Interp {
    temps: Array2<f32>,
    region_shape: (usize, usize),
}

impl Interp {
    pub fn new(temps: Array2<f32>, region_shape: (usize, usize)) -> Self {
        Self {
            temps,
            region_shape,
        }
    }

    pub fn interp_single_point<'a>(&'a self, pos: usize) -> ArrayView1<'a, f32> {
        let (cal_h, cal_w) = self.region_shape;
        let pos = match self.temps.nrows() {
            h if h == cal_w => pos % cal_w,
            h if h == cal_h => pos / cal_w,
            _ => pos,
        };
        self.temps.row(pos)
    }

    pub fn interp_single_frame(&self, frame: usize) -> TLCResult<Array2<f32>> {
        let (cal_h, cal_w) = self.region_shape;
        let col = self.temps.column(frame);
        let single_frame = match self.temps.nrows() {
            h if h == cal_w => col
                .broadcast((cal_h, cal_w))
                .ok_or(err!(UnKnown, "参考温度矩阵形状转换失败"))?
                .to_owned(),
            h if h == cal_h => col
                .to_owned()
                .into_shape((cal_h, 1))
                .map_err(|err| err!(UnKnown, err))?
                .broadcast((cal_h, cal_w))
                .ok_or(err!(UnKnown, "参考温度矩阵形状转换失败"))?
                .to_owned(),
            _ => col
                .to_owned()
                .into_shape(self.region_shape)
                .map_err(|err| err!(UnKnown, err))?
                .to_owned(),
        };

        Ok(single_frame)
    }
}

/// interpolation of reference temperature matrix
pub fn interp(
    t2d: ArrayView2<f32>,
    interp_method: InterpMethod,
    tc_pos: &[(i32, i32)],
    tl_pos: (usize, usize),
    region_shape: (usize, usize),
) -> TLCResult<Interp> {
    match interp_method {
        InterpMethod::Bilinear(_) | InterpMethod::BilinearExtra(_) => {
            interp_bilinear(t2d, interp_method, region_shape, tc_pos, tl_pos)
        }
        _ => interp1d(t2d, interp_method, region_shape, tc_pos, tl_pos),
    }
}

fn interp1d(
    t2d: ArrayView2<f32>,
    interp_method: InterpMethod,
    region_shape: (usize, usize),
    tc_pos: &[(i32, i32)],
    tl_pos: (usize, usize),
) -> TLCResult<Interp> {
    let (cal_h, cal_w) = region_shape;
    let frame_num = t2d.ncols();

    let (interp_len, tc_pos): (_, Vec<_>) = match interp_method {
        InterpMethod::Horizontal | InterpMethod::HorizontalExtra => (
            cal_w,
            tc_pos.iter().map(|(_, x)| x - tl_pos.1 as i32).collect(),
        ),
        InterpMethod::Vertical | InterpMethod::VerticalExtra => (
            cal_h,
            tc_pos.iter().map(|(y, _)| y - tl_pos.0 as i32).collect(),
        ),
        _ => unreachable!(),
    };

    let do_extra = match interp_method {
        InterpMethod::HorizontalExtra | InterpMethod::VerticalExtra => true,
        _ => false,
    };

    let mut temps = Array2::zeros((interp_len, frame_num));

    temps
        .axis_iter_mut(Axis(0))
        .into_par_iter()
        .zip(0..interp_len)
        .try_for_each(|(mut row, pos)| -> TLCResult<()> {
            let pos = pos as i32;
            let (mut li, mut ri) = (0, 1);
            while pos >= tc_pos[ri] && ri < tc_pos.len() - 1 {
                li += 1;
                ri += 1;
            }
            let (l, r) = (tc_pos[li], tc_pos[ri]);
            let (l_temps, r_temps) = (t2d.row(li), t2d.row(ri));
            let l_temps = l_temps.as_slice_memory_order().ok_or(err!())?;
            let r_temps = r_temps.as_slice_memory_order().ok_or(err!())?;

            let pos = if do_extra { pos } else { pos.max(l).min(r) };

            let row = row.as_slice_memory_order_mut().ok_or(err!())?;
            let mut frame = 0;
            while frame + f32x8::lanes() < frame_num {
                let lv = f32x8::from_slice_unaligned(&l_temps[frame..]);
                let rv = f32x8::from_slice_unaligned(&r_temps[frame..]);
                let v8 = (lv * (r - pos) as f32 + rv * (pos - l) as f32) / (r - l) as f32;
                {
                    v8.write_to_slice_unaligned(&mut row[frame..])
                };
                frame += f32x8::lanes();
            }
            while frame < frame_num {
                let (lv, rv) = (l_temps[frame], r_temps[frame]);
                row[frame] = (lv * (r - pos) as f32 + rv * (pos - l) as f32) / (r - l) as f32;
                frame += 1;
            }

            Ok(())
        })?;

    Ok(Interp::new(temps, region_shape))
}

fn interp_bilinear(
    t2d: ArrayView2<f32>,
    interp_method: InterpMethod,
    region_shape: (usize, usize),
    tc_pos: &[(i32, i32)],
    tl_pos: (usize, usize),
) -> TLCResult<Interp> {
    let (tc_shape, do_extra) = match interp_method {
        InterpMethod::Bilinear(tc_shape) => (tc_shape, false),
        InterpMethod::BilinearExtra(tc_shape) => (tc_shape, true),
        _ => unreachable!(),
    };
    let (tc_h, tc_w) = tc_shape;
    if tc_h * tc_w != tc_pos.len() {
        return Err(err!(UnKnown, "检查热电偶位置设置"));
    }
    let tc_x: Vec<_> = tc_pos
        .iter()
        .take(tc_w)
        .map(|(_, x)| x - tl_pos.1 as i32)
        .collect();
    let tc_y: Vec<_> = tc_pos
        .iter()
        .step_by(tc_w)
        .take(tc_h)
        .map(|(y, _)| y - tl_pos.0 as i32)
        .collect();

    let (cal_h, cal_w) = region_shape;
    let frame_num = t2d.ncols();
    let pix_num = cal_h * cal_w;
    let mut temps = Array2::zeros((pix_num, frame_num));

    temps
        .axis_iter_mut(Axis(0))
        .into_par_iter()
        .zip(0..pix_num)
        .try_for_each(|(mut row, pos)| -> TLCResult<()> {
            let x = (pos % cal_w) as i32;
            let y = (pos / cal_w) as i32;
            let (mut yi0, mut yi1) = (0, 1);
            while y >= tc_y[yi1] && yi1 < tc_h - 1 {
                yi0 += 1;
                yi1 += 1;
            }
            let (mut xi0, mut xi1) = (0, 1);
            while x >= tc_x[xi1] && xi1 < tc_w - 1 {
                xi0 += 1;
                xi1 += 1;
            }
            let (x0, x1, y0, y1) = (tc_x[xi0], tc_x[xi1], tc_y[yi0], tc_y[yi1]);
            let t00 = t2d.row(tc_w * yi0 + xi0);
            let t01 = t2d.row(tc_w * yi0 + xi1);
            let t10 = t2d.row(tc_w * yi1 + xi0);
            let t11 = t2d.row(tc_w * yi1 + xi1);
            let t00 = t00.as_slice_memory_order().ok_or(err!())?;
            let t01 = t01.as_slice_memory_order().ok_or(err!())?;
            let t10 = t10.as_slice_memory_order().ok_or(err!())?;
            let t11 = t11.as_slice_memory_order().ok_or(err!())?;

            let x = if do_extra { x } else { x.max(x0).min(x1) };
            let y = if do_extra { y } else { y.max(y0).min(y1) };

            let row = row.as_slice_memory_order_mut().ok_or(err!())?;
            let mut frame = 0;
            while frame + f32x8::lanes() < frame_num {
                let v00 = f32x8::from_slice_unaligned(&t00[frame..]);
                let v01 = f32x8::from_slice_unaligned(&t01[frame..]);
                let v10 = f32x8::from_slice_unaligned(&t10[frame..]);
                let v11 = f32x8::from_slice_unaligned(&t11[frame..]);
                let v8 = (v00 * (x1 - x) as f32 * (y1 - y) as f32
                    + v01 * (x - x0) as f32 * (y1 - y) as f32
                    + v10 * (x1 - x) as f32 * (y - y0) as f32
                    + v11 * (x - x0) as f32 * (y - y0) as f32)
                    / (x1 - x0) as f32
                    / (y1 - y0) as f32;
                v8.write_to_slice_unaligned(&mut row[frame..]);
                frame += f32x8::lanes();
            }
            while frame < frame_num {
                let v00 = t00[frame];
                let v01 = t01[frame];
                let v10 = t10[frame];
                let v11 = t11[frame];
                row[frame] = (v00 * (x1 - x) as f32 * (y1 - y) as f32
                    + v01 * (x - x0) as f32 * (y1 - y) as f32
                    + v10 * (x1 - x) as f32 * (y - y0) as f32
                    + v11 * (x - x0) as f32 * (y - y0) as f32)
                    / (x1 - x0) as f32
                    / (y1 - y0) as f32;
                frame += 1;
            }

            Ok(())
        })?;

    Ok(Interp::new(temps, region_shape))
}
