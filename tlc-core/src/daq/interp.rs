use anyhow::{anyhow, Result};
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use packed_simd::f64x4;
use serde::{Deserialize, Serialize};

use crate::daq::Thermocouple;

#[derive(Debug, Clone, PartialEq)]
pub struct InterpId {
    pub daq_id: DaqId,
    pub start_row: usize,
    pub cal_num: usize,
    pub area: (u32, u32, u32, u32),
    pub interp_method: InterpMethod,
    pub thermocouples: Vec<Thermocouple>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterpMethod {
    Horizontal,
    HorizontalExtra,
    Vertical,
    VerticalExtra,
    Bilinear(usize, usize),
    BilinearExtra(usize, usize),
}

use InterpMethod::*;

use super::DaqId;

pub fn interp(interp_meta: &InterpId, daq_raw: ArcArray2<f64>) -> Result<Interpolator> {
    let InterpId {
        start_row,
        cal_num,
        area,
        interp_method,
        ref thermocouples,
        ..
    } = *interp_meta;

    let mut temp2 = Array2::zeros((thermocouples.len(), cal_num));
    daq_raw
        .rows()
        .into_iter()
        .skip(start_row)
        .take(cal_num)
        .zip(temp2.columns_mut())
        .try_for_each(|(daq_row, mut col)| {
            thermocouples
                .iter()
                .zip(col.iter_mut())
                .try_for_each(|(tc, t)| -> Result<()> {
                    *t = *daq_row.get(tc.column_index).ok_or_else(|| {
                        anyhow!(
                            "thermocouple column index out of range: {} > {}",
                            tc.column_index,
                            daq_row.len()
                        )
                    })?;
                    Ok(())
                })
        })?;

    let data = match interp_method {
        Bilinear(..) | BilinearExtra(..) => interp2(temp2, interp_method, area, thermocouples),
        _ => interp1(temp2, interp_method, area, thermocouples),
    };

    Ok(Interpolator {
        interp_method,
        shape: (area.2, area.3),
        data: data.into_shared(),
    })
}

impl Interpolator {
    pub fn interp_frame(&self, frame_index: usize) -> Result<Array2<f64>> {
        let (cal_h, cal_w) = self.shape;
        let (cal_h, cal_w) = (cal_h as usize, cal_w as usize);
        let temp1 = self.data.column(frame_index);
        let temp2 = match self.interp_method {
            Horizontal | HorizontalExtra => temp1
                .broadcast((cal_h, cal_w))
                .ok_or_else(|| anyhow!("failed to broadcast"))?
                .to_owned(),
            Vertical | VerticalExtra => temp1
                .to_owned()
                .into_shape((cal_h, 1))?
                .broadcast((cal_h, cal_w))
                .ok_or_else(|| anyhow!("failed to broadcast"))?
                .to_owned(),
            Bilinear(..) | BilinearExtra(..) => temp1.to_owned().into_shape((cal_h, cal_w))?,
        };

        Ok(temp2)
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
    temp2: Array2<f64>,
    interp_method: InterpMethod,
    area: (u32, u32, u32, u32),
    thermocouples: &[Thermocouple],
) -> Array2<f64> {
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let (tl_y, tl_x, cal_h, cal_w) = (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
    let cal_num = temp2.ncols();

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
            let (l_temps, r_temps) = (temp2.row(li), temp2.row(ri));
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

    data
}

fn interp2(
    temp2: Array2<f64>,
    interp_method: InterpMethod,
    area: (u32, u32, u32, u32),
    thermocouples: &[Thermocouple],
) -> Array2<f64> {
    let (tc_h, tc_w, do_extra) = match interp_method {
        Bilinear(tc_h, tc_w) => (tc_h, tc_w, false),
        BilinearExtra(tc_h, tc_w) => (tc_h, tc_w, true),
        _ => unreachable!(),
    };
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let (tl_y, tl_x, cal_h, cal_w) = (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
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
            let t00 = temp2.row(tc_w * yi0 + xi0);
            let t01 = temp2.row(tc_w * yi0 + xi1);
            let t10 = temp2.row(tc_w * yi1 + xi0);
            let t11 = temp2.row(tc_w * yi1 + xi1);
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

    data
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn test_interpolate_horizontal_no_extra() {
        let daq_raw = array![
            // 3 points 2 frames.
            [1.0, 2.0, 3.0],
            [5.0, 6.0, 7.0],
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: Horizontal,
            thermocouples: thermocouples_from_slice(&[
                // 1 2 3
                (10, 10),
                (10, 11),
                (10, 12),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [1.0, 1.0, 2.0, 3.0, 3.0]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [5.0, 5.0, 6.0, 7.0, 7.0]
            ]
        );
    }

    #[test]
    fn test_interpolate_horizontal_extra() {
        let daq_raw = array![
            // 3 points 2 frames.
            [1.0, 2.0, 3.0],
            [5.0, 6.0, 7.0],
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: HorizontalExtra,
            thermocouples: thermocouples_from_slice(&[
                // 1 2 3
                (10, 10),
                (10, 11),
                (10, 12),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [0.0, 1.0, 2.0, 3.0, 4.0],
                [0.0, 1.0, 2.0, 3.0, 4.0],
                [0.0, 1.0, 2.0, 3.0, 4.0],
                [0.0, 1.0, 2.0, 3.0, 4.0],
                [0.0, 1.0, 2.0, 3.0, 4.0]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [4.0, 5.0, 6.0, 7.0, 8.0],
                [4.0, 5.0, 6.0, 7.0, 8.0],
                [4.0, 5.0, 6.0, 7.0, 8.0],
                [4.0, 5.0, 6.0, 7.0, 8.0],
                [4.0, 5.0, 6.0, 7.0, 8.0]
            ]
        );
    }

    #[test]
    fn test_interpolate_vertical_no_extra() {
        let daq_raw = array![
            // 2 points 2 frames.
            [1.0, 2.0],
            [5.0, 6.0],
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: Vertical,
            thermocouples: thermocouples_from_slice(&[
                // 1
                // 2
                (10, 10),
                (12, 10),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [1.0, 1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0, 1.0],
                [1.5, 1.5, 1.5, 1.5, 1.5],
                [2.0, 2.0, 2.0, 2.0, 2.0],
                [2.0, 2.0, 2.0, 2.0, 2.0]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [5.0, 5.0, 5.0, 5.0, 5.0],
                [5.0, 5.0, 5.0, 5.0, 5.0],
                [5.5, 5.5, 5.5, 5.5, 5.5],
                [6.0, 6.0, 6.0, 6.0, 6.0],
                [6.0, 6.0, 6.0, 6.0, 6.0]
            ]
        );
    }

    #[test]
    fn test_interpolate_vertical_extra() {
        let daq_raw = array![
            // 2 points 2 frames.
            [1.0, 2.0],
            [5.0, 6.0],
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: VerticalExtra,
            thermocouples: thermocouples_from_slice(&[
                // 1
                // 2
                (10, 10),
                (12, 10),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [0.5, 0.5, 0.5, 0.5, 0.5],
                [1.0, 1.0, 1.0, 1.0, 1.0],
                [1.5, 1.5, 1.5, 1.5, 1.5],
                [2.0, 2.0, 2.0, 2.0, 2.0],
                [2.5, 2.5, 2.5, 2.5, 2.5]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [4.5, 4.5, 4.5, 4.5, 4.5],
                [5.0, 5.0, 5.0, 5.0, 5.0],
                [5.5, 5.5, 5.5, 5.5, 5.5],
                [6.0, 6.0, 6.0, 6.0, 6.0],
                [6.5, 6.5, 6.5, 6.5, 6.5]
            ]
        );
    }

    #[test]
    fn test_interpolate_bilinear_no_extra() {
        let daq_raw = array![
            // 6 points 2 frames.
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            [5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: Bilinear(2, 3),
            // 1 2 3
            // 4 5 6
            thermocouples: thermocouples_from_slice(&[
                (10, 10),
                (10, 11),
                (10, 12),
                (12, 10),
                (12, 11),
                (12, 12),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [1.0, 1.0, 2.0, 3.0, 3.0],
                [2.5, 2.5, 3.5, 4.5, 4.5],
                [4.0, 4.0, 5.0, 6.0, 6.0],
                [4.0, 4.0, 5.0, 6.0, 6.0]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [5.0, 5.0, 6.0, 7.0, 7.0],
                [6.5, 6.5, 7.5, 8.5, 8.5],
                [8.0, 8.0, 9.0, 10.0, 10.0],
                [8.0, 8.0, 9.0, 10.0, 10.0]
            ]
        );
    }

    #[test]
    fn test_interpolate_bilinear_extra() {
        let daq_raw = array![
            // 6 points 2 frames.
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            [5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        ];
        let interp_meta = InterpId {
            daq_id: DaqId::default(),
            start_row: 0,
            cal_num: 2,
            area: (9, 9, 5, 5),
            interp_method: BilinearExtra(2, 3),
            thermocouples: thermocouples_from_slice(&[
                // 1 2 3
                // 4 5 6
                (10, 10),
                (10, 11),
                (10, 12),
                (12, 10),
                (12, 11),
                (12, 12),
            ]),
        };
        let interpolator = interp(&interp_meta, daq_raw.into_shared()).unwrap();
        assert_relative_eq!(
            interpolator.interp_frame(0).unwrap(),
            array![
                [-1.5, -0.5, 0.5, 1.5, 2.5],
                [0.0, 1.0, 2.0, 3.0, 4.0],
                [1.5, 2.5, 3.5, 4.5, 5.5],
                [3.0, 4.0, 5.0, 6.0, 7.0],
                [4.5, 5.5, 6.5, 7.5, 8.5]
            ]
        );
        assert_relative_eq!(
            interpolator.interp_frame(1).unwrap(),
            array![
                [2.5, 3.5, 4.5, 5.5, 6.5],
                [4.0, 5.0, 6.0, 7.0, 8.0],
                [5.5, 6.5, 7.5, 8.5, 9.5],
                [7.0, 8.0, 9.0, 10.0, 11.0],
                [8.5, 9.5, 10.5, 11.5, 12.5]
            ]
        );
    }

    fn thermocouples_from_slice(arr: &[(i32, i32)]) -> Vec<Thermocouple> {
        arr.iter()
            .enumerate()
            .map(|(column_index, &position)| Thermocouple {
                column_index,
                position,
            })
            .collect()
    }

    impl Default for Interpolator {
        fn default() -> Interpolator {
            Interpolator {
                interp_method: InterpMethod::Horizontal,
                shape: (100, 100),
                data: ArcArray2::zeros((100, 2000)),
            }
        }
    }
}
