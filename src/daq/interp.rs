#![allow(dead_code)]

use ndarray::{parallel::prelude::*, prelude::*, ArcArray2, Zip};
use serde::{Deserialize, Serialize};

use crate::daq::Thermocouple;
use InterpMethod::*;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum InterpMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear(u8, u8),
    BilinearExtra(u8, u8),
}

#[derive(Debug, Clone)]
pub struct Interpolator {
    interp_method: InterpMethod,
    shape: (u32, u32),
    /// horizontal: (cal_w, cal_num)
    /// vertical: (cal_h, cal_num)
    /// bilinear: (cal_h * cal_w, cal_num)
    data: ArcArray2<f64>,
}

impl Interpolator {
    pub fn new(
        start_row: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
        interp_method: InterpMethod,
        thermocouples: &[Thermocouple],
        daq_data: ArrayView2<f64>,
    ) -> Interpolator {
        assert!(thermocouples
            .iter()
            .all(|tc| tc.column_index < daq_data.ncols()));

        let mut temp2 = Array2::zeros((thermocouples.len(), cal_num));
        daq_data
            .rows()
            .into_iter()
            .skip(start_row)
            .take(cal_num)
            .zip(temp2.columns_mut())
            .for_each(|(daq_row, mut col)| {
                thermocouples
                    .iter()
                    .zip(col.iter_mut())
                    .for_each(|(tc, t)| *t = daq_row[tc.column_index])
            });

        let data = match interp_method {
            Bilinear(..) | BilinearExtra(..) => interp2(temp2, interp_method, area, thermocouples),
            Horizontal | HorizontalExtra | Vertical | VerticalExtra => {
                interp1(temp2.view(), interp_method, area, thermocouples)
            }
        };

        Interpolator {
            interp_method,
            shape: (area.2, area.3),
            data: data.into_shared(),
        }
    }

    pub fn interp_frame(&self, frame_index: usize) -> Array2<f64> {
        let (cal_h, cal_w) = (self.shape.0 as usize, self.shape.1 as usize);
        let temp1 = self.data.column(frame_index);
        match self.interp_method {
            Horizontal | HorizontalExtra => {
                assert_eq!(temp1.len(), cal_w, "horizontal interp stores x-axis values");
                temp1.broadcast((cal_h, cal_w)).unwrap().to_owned()
            }
            Vertical | VerticalExtra => {
                assert_eq!(temp1.len(), cal_h, "vertical interp stores y-axis values");
                temp1
                    .broadcast((cal_w, cal_h))
                    .unwrap()
                    .reversed_axes()
                    .to_owned()
            }
            Bilinear(..) | BilinearExtra(..) => {
                assert_eq!(temp1.len(), cal_h * cal_w);
                temp1.to_owned().into_shape((cal_h, cal_w)).unwrap()
            }
        }
    }

    /// point_index = y * w + x.
    pub fn interp_point(&self, point_index: usize) -> ArrayView1<f64> {
        let point_index = match self.interp_method {
            Horizontal | HorizontalExtra => point_index / self.shape.1 as usize,
            Vertical | VerticalExtra => point_index % self.shape.0 as usize,
            Bilinear(..) | BilinearExtra(..) => point_index,
        };
        self.data.row(point_index)
    }

    pub fn shape(&self) -> (u32, u32) {
        self.shape
    }
}

fn interp1(
    temp2: ArrayView2<f64>,
    interp_method: InterpMethod,
    area: (u32, u32, u32, u32),
    thermocouples: &[Thermocouple],
) -> Array2<f64> {
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let cal_num = temp2.ncols();

    let (interp_len, tc_x): (_, Vec<_>) = match interp_method {
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
    let mut data = Array2::zeros((interp_len as usize, cal_num));

    data.axis_iter_mut(Axis(0))
        .into_par_iter()
        .enumerate()
        .for_each(|(pos, row)| {
            let mut x = pos as i32;
            let (i0, i1) = find_range(&tc_x, x);
            let (x0, x1) = (tc_x[i0], tc_x[i1]);

            if matches!(interp_method, Horizontal | Vertical) {
                x = x.clamp(x0, x1)
            };

            Zip::from(row)
                .and(temp2.row(i0))
                .and(temp2.row(i1))
                .for_each(|v, v0, v1| {
                    *v = (v0 * (x1 - x) as f64 + v1 * (x - x0) as f64) / (x1 - x0) as f64
                });
        });

    data
}

fn interp2(
    temp2: Array2<f64>,
    interp_method: InterpMethod,
    area: (u32, u32, u32, u32),
    thermocouples: &[Thermocouple],
) -> Array2<f64> {
    let (tc_h, tc_w) = match interp_method {
        Bilinear(tc_h, tc_w) => (tc_h as usize, tc_w as usize),
        BilinearExtra(tc_h, tc_w) => (tc_h as usize, tc_w as usize),
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

    let cal_num = temp2.ncols();
    let pix_num = cal_h * cal_w;
    let mut data = Array2::zeros((pix_num as usize, cal_num));

    data.axis_iter_mut(Axis(0))
        .into_par_iter()
        .enumerate()
        .for_each(|(pos, row)| {
            let mut x = pos as i32 % cal_w as i32;
            let mut y = pos as i32 / cal_w as i32;

            let (yi0, yi1) = find_range(&tc_y, y);
            let (y0, y1) = (tc_y[yi0], tc_y[yi1]);
            let (xi0, xi1) = find_range(&tc_x, x);
            let (x0, x1) = (tc_x[xi0], tc_x[xi1]);

            if matches!(interp_method, Bilinear(..)) {
                x = x.clamp(x0, x1);
                y = y.clamp(y0, y1);
            }

            Zip::from(row)
                .and(temp2.row(tc_w * yi0 + xi0))
                .and(temp2.row(tc_w * yi0 + xi1))
                .and(temp2.row(tc_w * yi1 + xi0))
                .and(temp2.row(tc_w * yi1 + xi1))
                .for_each(|v, v00, v01, v10, v11| {
                    *v = (v00 * (x1 - x) as f64 * (y1 - y) as f64
                        + v01 * (x - x0) as f64 * (y1 - y) as f64
                        + v10 * (x1 - x) as f64 * (y - y0) as f64
                        + v11 * (x - x0) as f64 * (y - y0) as f64)
                        / (x1 - x0) as f64
                        / (y1 - y0) as f64;
                });
        });

    data
}

fn find_range(vs: &[i32], x: i32) -> (usize, usize) {
    assert!(vs.len() > 1);
    let mut i1 = 1;
    while i1 < vs.len() - 1 && x >= vs[i1] {
        i1 += 1;
    }
    let i0 = i1 - 1;
    (i0, i1)
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn test_interp() {
        for (interp_method, thermocouples, daq_data, frame0, frame1) in [
            (
                Horizontal,
                &[
                    // 1 2 3
                    (10, 10),
                    (10, 11),
                    (10, 12),
                ][..],
                array![
                    // 3 points 2 frames.
                    [1.0, 2.0, 3.0],
                    [5.0, 6.0, 7.0],
                ],
                array![
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [1.0, 1.0, 2.0, 3.0, 3.0]
                ],
                array![
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [5.0, 5.0, 6.0, 7.0, 7.0]
                ],
            ),
            (
                HorizontalExtra,
                &[
                    // 1 2 3
                    (10, 10),
                    (10, 11),
                    (10, 12),
                ],
                array![
                    // 3 points 2 frames.
                    [1.0, 2.0, 3.0],
                    [5.0, 6.0, 7.0],
                ],
                array![
                    [0.0, 1.0, 2.0, 3.0, 4.0],
                    [0.0, 1.0, 2.0, 3.0, 4.0],
                    [0.0, 1.0, 2.0, 3.0, 4.0],
                    [0.0, 1.0, 2.0, 3.0, 4.0],
                    [0.0, 1.0, 2.0, 3.0, 4.0]
                ],
                array![
                    [4.0, 5.0, 6.0, 7.0, 8.0],
                    [4.0, 5.0, 6.0, 7.0, 8.0],
                    [4.0, 5.0, 6.0, 7.0, 8.0],
                    [4.0, 5.0, 6.0, 7.0, 8.0],
                    [4.0, 5.0, 6.0, 7.0, 8.0]
                ],
            ),
            (
                Vertical,
                &[
                    // 1
                    // 2
                    (10, 10),
                    (12, 10),
                ],
                array![
                    // 2 points 2 frames.
                    [1.0, 2.0],
                    [5.0, 6.0],
                ],
                array![
                    [1.0, 1.0, 1.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0, 1.0],
                    [1.5, 1.5, 1.5, 1.5, 1.5],
                    [2.0, 2.0, 2.0, 2.0, 2.0],
                    [2.0, 2.0, 2.0, 2.0, 2.0]
                ],
                array![
                    [5.0, 5.0, 5.0, 5.0, 5.0],
                    [5.0, 5.0, 5.0, 5.0, 5.0],
                    [5.5, 5.5, 5.5, 5.5, 5.5],
                    [6.0, 6.0, 6.0, 6.0, 6.0],
                    [6.0, 6.0, 6.0, 6.0, 6.0]
                ],
            ),
            (
                VerticalExtra,
                &[
                    // 1
                    // 2
                    (10, 10),
                    (12, 10),
                ],
                array![
                    // 2 points 2 frames.
                    [1.0, 2.0],
                    [5.0, 6.0],
                ],
                array![
                    [0.5, 0.5, 0.5, 0.5, 0.5],
                    [1.0, 1.0, 1.0, 1.0, 1.0],
                    [1.5, 1.5, 1.5, 1.5, 1.5],
                    [2.0, 2.0, 2.0, 2.0, 2.0],
                    [2.5, 2.5, 2.5, 2.5, 2.5]
                ],
                array![
                    [4.5, 4.5, 4.5, 4.5, 4.5],
                    [5.0, 5.0, 5.0, 5.0, 5.0],
                    [5.5, 5.5, 5.5, 5.5, 5.5],
                    [6.0, 6.0, 6.0, 6.0, 6.0],
                    [6.5, 6.5, 6.5, 6.5, 6.5]
                ],
            ),
            (
                Bilinear(2, 3),
                &[
                    // 1 2 3
                    // 4 5 6
                    (10, 10),
                    (10, 11),
                    (10, 12),
                    (12, 10),
                    (12, 11),
                    (12, 12),
                ],
                array![
                    // 6 points 2 frames.
                    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                    [5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
                ],
                array![
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [1.0, 1.0, 2.0, 3.0, 3.0],
                    [2.5, 2.5, 3.5, 4.5, 4.5],
                    [4.0, 4.0, 5.0, 6.0, 6.0],
                    [4.0, 4.0, 5.0, 6.0, 6.0]
                ],
                array![
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [5.0, 5.0, 6.0, 7.0, 7.0],
                    [6.5, 6.5, 7.5, 8.5, 8.5],
                    [8.0, 8.0, 9.0, 10.0, 10.0],
                    [8.0, 8.0, 9.0, 10.0, 10.0]
                ],
            ),
            (
                BilinearExtra(2, 3),
                &[
                    // 1 2 3
                    // 4 5 6
                    (10, 10),
                    (10, 11),
                    (10, 12),
                    (12, 10),
                    (12, 11),
                    (12, 12),
                ],
                array![
                    // 6 points 2 frames.
                    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                    [5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
                ],
                array![
                    [-1.5, -0.5, 0.5, 1.5, 2.5],
                    [0.0, 1.0, 2.0, 3.0, 4.0],
                    [1.5, 2.5, 3.5, 4.5, 5.5],
                    [3.0, 4.0, 5.0, 6.0, 7.0],
                    [4.5, 5.5, 6.5, 7.5, 8.5]
                ],
                array![
                    [2.5, 3.5, 4.5, 5.5, 6.5],
                    [4.0, 5.0, 6.0, 7.0, 8.0],
                    [5.5, 6.5, 7.5, 8.5, 9.5],
                    [7.0, 8.0, 9.0, 10.0, 11.0],
                    [8.5, 9.5, 10.5, 11.5, 12.5]
                ],
            ),
        ] {
            let thermocouples: Vec<_> = thermocouples
                .iter()
                .enumerate()
                .map(|(column_index, &position)| Thermocouple {
                    column_index,
                    position,
                })
                .collect();
            let interpolator = Interpolator::new(
                0,
                2,
                (9, 9, 5, 5),
                interp_method,
                &thermocouples,
                daq_data.view(),
            );
            assert_relative_eq!(interpolator.interp_frame(0), frame0);
            assert_relative_eq!(interpolator.interp_frame(1), frame1);
        }
    }
}
