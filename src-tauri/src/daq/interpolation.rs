use ndarray::{parallel::prelude::*, prelude::*};
use packed_simd::f64x4;
use serde::{Deserialize, Serialize};

use crate::daq::Thermocouple;

use InterpolationMethod::*;

#[derive(Debug)]
pub struct Temperature2 {
    interpolation_method: InterpolationMethod,

    shape: (usize, usize),

    /// horizontal: (cal_w, cal_num)
    /// vertical: (cal_h, cal_num)
    /// bilinear: (cal_h * cal_w, cal_num)
    inner: Array2<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum InterpolationMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear(usize, usize),
    BilinearExtra(usize, usize),
}

impl Temperature2 {
    pub fn new(
        daq_data: ArrayView2<f64>,
        interp_method: InterpolationMethod,
        area: (usize, usize, usize, usize),
        thermocouples: &[Thermocouple],
    ) -> Self {
        match interp_method {
            Bilinear(_, _) | BilinearExtra(_, _) => {
                interpolator2(daq_data, interp_method, area, thermocouples)
            }
            _ => interpolator1(daq_data, interp_method, area, thermocouples),
        }
    }
}

fn interpolator1(
    daq_data: ArrayView2<f64>,
    interp_method: InterpolationMethod,
    area: (usize, usize, usize, usize),
    thermocouples: &[Thermocouple],
) -> Temperature2 {
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let frame_num = daq_data.ncols();

    let (interp_len, tc_pos): (_, Vec<_>) = match interp_method {
        InterpolationMethod::Horizontal | InterpolationMethod::HorizontalExtra => (
            cal_w,
            thermocouples
                .iter()
                .map(|tc| tc.position.1 - tl_x as i32)
                .collect(),
        ),
        InterpolationMethod::Vertical | InterpolationMethod::VerticalExtra => (
            cal_h,
            thermocouples
                .iter()
                .map(|tc| tc.position.0 - tl_y as i32)
                .collect(),
        ),
        _ => unreachable!(),
    };

    let do_extra = matches!(interp_method, HorizontalExtra | VerticalExtra);
    let mut temperature2 = Array2::zeros((interp_len, frame_num));

    temperature2
        .axis_iter_mut(Axis(0))
        .into_par_iter()
        .zip(0..interp_len)
        .for_each(|(mut row, pos)| {
            let pos = pos as i32;
            let (mut li, mut ri) = (0, 1);
            while pos >= tc_pos[ri] && ri < tc_pos.len() - 1 {
                li += 1;
                ri += 1;
            }
            let (l, r) = (tc_pos[li], tc_pos[ri]);
            let (l_temps, r_temps) = (daq_data.row(li), daq_data.row(ri));
            let l_temps = l_temps.as_slice_memory_order().unwrap();
            let r_temps = r_temps.as_slice_memory_order().unwrap();

            let pos = if do_extra { pos } else { pos.max(l).min(r) };

            let row = row.as_slice_memory_order_mut().unwrap();
            let mut frame = 0;
            while frame + f64x4::lanes() < frame_num {
                let lv = f64x4::from_slice_unaligned(&l_temps[frame..]);
                let rv = f64x4::from_slice_unaligned(&r_temps[frame..]);
                let v8 = (lv * (r - pos) as f64 + rv * (pos - l) as f64) / (r - l) as f64;
                v8.write_to_slice_unaligned(&mut row[frame..]);
                frame += f64x4::lanes();
            }
            while frame < frame_num {
                let (lv, rv) = (l_temps[frame], r_temps[frame]);
                row[frame] = (lv * (r - pos) as f64 + rv * (pos - l) as f64) / (r - l) as f64;
                frame += 1;
            }
        });

    Temperature2 {
        interpolation_method: interp_method,
        shape: (cal_h, cal_w),
        inner: temperature2,
    }
}

fn interpolator2(
    daq_data: ArrayView2<f64>,
    interp_method: InterpolationMethod,
    area: (usize, usize, usize, usize),
    thermocouples: &[Thermocouple],
) -> Temperature2 {
    let (tc_h, tc_w, do_extra) = match interp_method {
        Bilinear(tc_h, tc_w) => (tc_h, tc_w, false),
        BilinearExtra(tc_h, tc_w) => (tc_h, tc_w, true),
        _ => unreachable!(),
    };
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let tc_x: Vec<_> = thermocouples
        .iter()
        .take(tc_w)
        .map(|tc| tc.position.1 - tl_x as i32)
        .collect();
    let tc_y: Vec<_> = thermocouples
        .iter()
        .step_by(tc_w)
        .take(tc_h)
        .map(|tc| tc.position.0 - tl_y as i32)
        .collect();

    let frame_num = daq_data.ncols();
    let pix_num = cal_h * cal_w;
    let mut temperature2 = Array2::zeros((pix_num, frame_num));

    temperature2
        .axis_iter_mut(Axis(0))
        .into_par_iter()
        .zip(0..pix_num)
        .for_each(|(mut row, pos)| {
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
            let t00 = daq_data.row(tc_w * yi0 + xi0);
            let t01 = daq_data.row(tc_w * yi0 + xi1);
            let t10 = daq_data.row(tc_w * yi1 + xi0);
            let t11 = daq_data.row(tc_w * yi1 + xi1);
            let t00 = t00.as_slice_memory_order().unwrap();
            let t01 = t01.as_slice_memory_order().unwrap();
            let t10 = t10.as_slice_memory_order().unwrap();
            let t11 = t11.as_slice_memory_order().unwrap();

            let x = if do_extra { x } else { x.max(x0).min(x1) };
            let y = if do_extra { y } else { y.max(y0).min(y1) };

            let row = row.as_slice_memory_order_mut().unwrap();
            let mut frame = 0;
            while frame + f64x4::lanes() < frame_num {
                let v00 = f64x4::from_slice_unaligned(&t00[frame..]);
                let v01 = f64x4::from_slice_unaligned(&t01[frame..]);
                let v10 = f64x4::from_slice_unaligned(&t10[frame..]);
                let v11 = f64x4::from_slice_unaligned(&t11[frame..]);
                let v8 = (v00 * (x1 - x) as f64 * (y1 - y) as f64
                    + v01 * (x - x0) as f64 * (y1 - y) as f64
                    + v10 * (x1 - x) as f64 * (y - y0) as f64
                    + v11 * (x - x0) as f64 * (y - y0) as f64)
                    / (x1 - x0) as f64
                    / (y1 - y0) as f64;
                v8.write_to_slice_unaligned(&mut row[frame..]);
                frame += f64x4::lanes();
            }
            while frame < frame_num {
                let v00 = t00[frame];
                let v01 = t01[frame];
                let v10 = t10[frame];
                let v11 = t11[frame];
                row[frame] = (v00 * (x1 - x) as f64 * (y1 - y) as f64
                    + v01 * (x - x0) as f64 * (y1 - y) as f64
                    + v10 * (x1 - x) as f64 * (y - y0) as f64
                    + v11 * (x - x0) as f64 * (y - y0) as f64)
                    / (x1 - x0) as f64
                    / (y1 - y0) as f64;
                frame += 1;
            }
        });

    Temperature2 {
        interpolation_method: interp_method,
        shape: (cal_h, cal_w),
        inner: temperature2,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_interp_bilinear() {
        let daq_data = array![[1.], [2.], [3.], [4.], [5.], [6.]];
        println!("{:?}", daq_data.shape());
        let interp_method = BilinearExtra(2, 3);
        let thermocouples: Vec<Thermocouple> =
            [(10, 10), (10, 15), (10, 20), (20, 10), (20, 15), (20, 20)]
                .iter()
                .map(|&position| Thermocouple {
                    column_index: 0,
                    position,
                })
                .collect();
        let area = (8, 8, 14, 14);

        let _interpolator = Temperature2::new(daq_data.view(), interp_method, area, &thermocouples);
        todo!()
    }
}
