use anyhow::{anyhow, Result};
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use packed_simd::f64x4;
use serde::{Deserialize, Serialize};

use crate::daq::Thermocouple;

use InterpolationMethod::*;

#[derive(Debug, Clone)]
pub struct Interpolator {
    interpolation_method: InterpolationMethod,
    shape: (usize, usize),
    /// horizontal: (cal_w, cal_num)
    /// vertical: (cal_h, cal_num)
    /// bilinear: (cal_h * cal_w, cal_num)
    data: ArcArray2<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum InterpolationMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear(usize, usize),
    BilinearExtra(usize, usize),
}

impl Interpolator {
    pub fn new(
        temperature2: Array2<f64>,
        interpolation_method: InterpolationMethod,
        area: (usize, usize, usize, usize),
        thermocouples: &[Thermocouple],
    ) -> Self {
        match interpolation_method {
            Bilinear(..) | BilinearExtra(..) => {
                interpolator2(temperature2, interpolation_method, area, thermocouples)
            }
            _ => interpolator1(temperature2, interpolation_method, area, thermocouples),
        }
    }

    pub fn interpolate_single_frame(&self, frame_index: usize) -> Result<Array2<f64>> {
        let (cal_h, cal_w) = self.shape;
        let temperature_flattened = self.data.column(frame_index);
        let temperature_distribution = match self.interpolation_method {
            Horizontal | HorizontalExtra => temperature_flattened
                .broadcast((cal_h, cal_w))
                .ok_or_else(|| anyhow!("failed to broadcast"))?
                .to_owned(),
            Vertical | VerticalExtra => temperature_flattened
                .to_owned()
                .into_shape((cal_h, 1))?
                .broadcast((cal_h, cal_w))
                .ok_or_else(|| anyhow!("failed to broadcast"))?
                .to_owned(),
            Bilinear(..) | BilinearExtra(..) => {
                temperature_flattened.to_owned().into_shape(self.shape)?
            }
        };

        Ok(temperature_distribution)
    }

    /// point_index = y * w + x.
    pub fn interpolate_single_point(&self, point_index: usize) -> ArrayView1<f64> {
        let point_index = match self.interpolation_method {
            Horizontal | HorizontalExtra => point_index / self.shape.1,
            Vertical | VerticalExtra => point_index % self.shape.0,
            Bilinear(..) | BilinearExtra(..) => point_index,
        };

        self.data.row(point_index)
    }
}

fn interpolator1(
    temperature2: Array2<f64>,
    interp_method: InterpolationMethod,
    area: (usize, usize, usize, usize),
    thermocouples: &[Thermocouple],
) -> Interpolator {
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let cal_num = temperature2.ncols();

    let (interp_len, tc_pos): (_, Vec<_>) = match interp_method {
        Horizontal | HorizontalExtra => (
            cal_w,
            thermocouples
                .iter()
                .map(|tc| tc.position.1 - tl_x as i32)
                .collect(),
        ),
        Vertical | VerticalExtra => (
            cal_h,
            thermocouples
                .iter()
                .map(|tc| tc.position.0 - tl_y as i32)
                .collect(),
        ),
        _ => unreachable!(),
    };

    let do_extra = matches!(interp_method, HorizontalExtra | VerticalExtra);
    let mut data = Array2::zeros((interp_len, cal_num));

    data.axis_iter_mut(Axis(0))
        .into_par_iter()
        .enumerate()
        .for_each(|(pos, mut row)| {
            let pos = pos as i32;
            let (mut li, mut ri) = (0, 1);
            while pos >= tc_pos[ri] && ri < tc_pos.len() - 1 {
                li += 1;
                ri += 1;
            }
            let (l, r) = (tc_pos[li], tc_pos[ri]);
            let (l_temps, r_temps) = (temperature2.row(li), temperature2.row(ri));
            let l_temps = l_temps.as_slice_memory_order().unwrap();
            let r_temps = r_temps.as_slice_memory_order().unwrap();

            let pos = if do_extra { pos } else { pos.clamp(l, r) };

            let row = row.as_slice_memory_order_mut().unwrap();
            let mut frame = 0;
            while frame + f64x4::lanes() < cal_num {
                let lv = f64x4::from_slice_unaligned(&l_temps[frame..]);
                let rv = f64x4::from_slice_unaligned(&r_temps[frame..]);
                let v4 = (lv * (r - pos) as f64 + rv * (pos - l) as f64) / (r - l) as f64;
                v4.write_to_slice_unaligned(&mut row[frame..]);
                frame += f64x4::lanes();
            }
            while frame < cal_num {
                let (lv, rv) = (l_temps[frame], r_temps[frame]);
                row[frame] = (lv * (r - pos) as f64 + rv * (pos - l) as f64) / (r - l) as f64;
                frame += 1;
            }
        });

    Interpolator {
        interpolation_method: interp_method,
        shape: (cal_h, cal_w),
        data: data.into_shared(),
    }
}

fn interpolator2(
    temperature2: Array2<f64>,
    interp_method: InterpolationMethod,
    area: (usize, usize, usize, usize),
    thermocouples: &[Thermocouple],
) -> Interpolator {
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

    let cal_num = temperature2.ncols();
    let pix_num = cal_h * cal_w;
    let mut data = Array2::zeros((pix_num, cal_num));

    data.axis_iter_mut(Axis(0))
        .into_par_iter()
        .enumerate()
        .for_each(|(pos, mut row)| {
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
            let t00 = temperature2.row(tc_w * yi0 + xi0);
            let t01 = temperature2.row(tc_w * yi0 + xi1);
            let t10 = temperature2.row(tc_w * yi1 + xi0);
            let t11 = temperature2.row(tc_w * yi1 + xi1);
            let t00 = t00.as_slice_memory_order().unwrap();
            let t01 = t01.as_slice_memory_order().unwrap();
            let t10 = t10.as_slice_memory_order().unwrap();
            let t11 = t11.as_slice_memory_order().unwrap();

            let x = if do_extra { x } else { x.clamp(x0, x1) };
            let y = if do_extra { y } else { y.clamp(y0, y1) };

            let row = row.as_slice_memory_order_mut().unwrap();
            let mut frame = 0;
            while frame + f64x4::lanes() < cal_num {
                let v00 = f64x4::from_slice_unaligned(&t00[frame..]);
                let v01 = f64x4::from_slice_unaligned(&t01[frame..]);
                let v10 = f64x4::from_slice_unaligned(&t10[frame..]);
                let v11 = f64x4::from_slice_unaligned(&t11[frame..]);
                let v4 = (v00 * (x1 - x) as f64 * (y1 - y) as f64
                    + v01 * (x - x0) as f64 * (y1 - y) as f64
                    + v10 * (x1 - x) as f64 * (y - y0) as f64
                    + v11 * (x - x0) as f64 * (y - y0) as f64)
                    / (x1 - x0) as f64
                    / (y1 - y0) as f64;
                v4.write_to_slice_unaligned(&mut row[frame..]);
                frame += f64x4::lanes();
            }
            while frame < cal_num {
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

    Interpolator {
        interpolation_method: interp_method,
        shape: (cal_h, cal_w),
        data: data.into_shared(),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_interpolate_horizontal_no_extra() {
        let temperature2 = array![[1.0, 5.0], [2.0, 6.0], [3.0, 7.0]];
        let interp_method = Horizontal;
        // 1 2 3
        let thermocouples: Vec<Thermocouple> = [(10, 10), (10, 11), (10, 12)]
            .iter()
            .enumerate()
            .map(|(column_index, &position)| Thermocouple {
                column_index,
                position,
            })
            .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }

    #[test]
    fn test_interpolate_horizontal_extra() {
        let temperature2 = array![[1.0, 5.0], [2.0, 6.0], [3.0, 7.0]];
        let interp_method = HorizontalExtra;
        // 1 2 3
        let thermocouples: Vec<Thermocouple> = [(10, 10), (10, 11), (10, 12)]
            .iter()
            .enumerate()
            .map(|(column_index, &position)| Thermocouple {
                column_index,
                position,
            })
            .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }

    #[test]
    fn test_interpolate_vertical_no_extra() {
        let temperature2 = array![[1.0, 5.0], [2.0, 6.0], [3.0, 7.0]];
        let interp_method = Vertical;
        // 1
        // 2
        let thermocouples: Vec<Thermocouple> = [(10, 10), (12, 10)]
            .iter()
            .enumerate()
            .map(|(column_index, &position)| Thermocouple {
                column_index,
                position,
            })
            .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }

    #[test]
    fn test_interpolate_vertical_extra() {
        let temperature2 = array![[1.0, 5.0], [2.0, 6.0], [3.0, 7.0]];
        let interp_method = VerticalExtra;
        // 1
        // 2
        let thermocouples: Vec<Thermocouple> = [(10, 10), (12, 10)]
            .iter()
            .enumerate()
            .map(|(column_index, &position)| Thermocouple {
                column_index,
                position,
            })
            .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }

    #[test]
    fn test_interpolate_bilinear_no_extra() {
        let temperature2 = array![
            [1.0, 5.0],
            [2.0, 6.0],
            [3.0, 7.0],
            [4.0, 8.0],
            [5.0, 9.0],
            [6.0, 10.0]
        ];
        let interp_method = Bilinear(2, 3);
        // 1 2 3
        // 4 5 6
        let thermocouples: Vec<Thermocouple> =
            [(10, 10), (10, 11), (10, 12), (12, 10), (12, 11), (12, 12)]
                .iter()
                .enumerate()
                .map(|(column_index, &position)| Thermocouple {
                    column_index,
                    position,
                })
                .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }

    #[test]
    fn test_interpolate_bilinear_extra() {
        let temperature2 = array![
            [1.0, 5.0],
            [2.0, 6.0],
            [3.0, 7.0],
            [4.0, 8.0],
            [5.0, 9.0],
            [6.0, 10.0]
        ];
        let interp_method = BilinearExtra(2, 3);
        // 1 2 3
        // 4 5 6
        let thermocouples: Vec<Thermocouple> =
            [(10, 10), (10, 11), (10, 12), (12, 10), (12, 11), (12, 12)]
                .iter()
                .enumerate()
                .map(|(column_index, &position)| Thermocouple {
                    column_index,
                    position,
                })
                .collect();
        let area = (9, 9, 5, 5);
        let interpolator = Interpolator::new(temperature2, interp_method, area, &thermocouples);
        println!("{:?}", interpolator.interpolate_single_frame(0).unwrap());
        println!("{:?}", interpolator.interpolate_single_frame(1).unwrap());
    }
}
