use median::Filter;

use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

use serde::{Deserialize, Serialize};

use packed_simd::f32x8;

use super::{error::TLCResult, TLCConfig, TLCData, Thermocouple};
use crate::awsl;

const SCALING: usize = 5;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum FilterMethod {
    No,
    Median(usize),
    Wavelet(f32),
}

impl Default for FilterMethod {
    fn default() -> Self {
        FilterMethod::No
    }
}

impl TLCData {
    /// 对Green值矩阵沿时间轴滤波
    pub fn filtering(&mut self) -> TLCResult<&mut Self> {
        if self.raw_g2d.is_none() {
            self.read_video()?;
        }

        let mut filtered_g2d = self.get_raw_g2d()?.to_owned();

        match self.config.filter_method {
            FilterMethod::Median(window_size) => {
                filtered_g2d
                    .axis_iter_mut(Axis(1))
                    .into_par_iter()
                    .for_each(|mut col| {
                        let mut filter = Filter::new(window_size);
                        col.iter_mut().for_each(|g| *g = filter.consume(*g))
                    });

                self.filtered_g2d.insert(filtered_g2d);
            }
            _ => {}
        }

        Ok(self)
    }

    /// 峰值检测
    pub fn detect_peak(&mut self) -> TLCResult<&mut Self> {
        if self.filtered_g2d.is_none() {
            self.filtering()?;
        }

        let filtered_g2d = self.get_filtered_g2d()?;
        let mut peak_frames = vec![0; filtered_g2d.ncols()];

        filtered_g2d
            .axis_iter(Axis(1))
            .into_par_iter()
            .zip(peak_frames.par_iter_mut())
            .try_for_each(|(col, p)| -> TLCResult<()> {
                *p = col
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, g)| *g)
                    .ok_or(awsl!("峰值检测出错"))?
                    .0;

                Ok(())
            })?;
        self.peak_frames.insert(peak_frames);

        Ok(self)
    }

    pub fn init_t2d(&mut self) -> TLCResult<&mut Self> {
        if self.daq.is_none() {
            self.read_daq()?;
        }

        let TLCConfig {
            ref thermocouples,
            frame_num,
            start_row,
            ref regulator,
            ..
        } = self.config;
        let mut t2d = Array2::zeros((thermocouples.len(), frame_num));

        for (daq_row, mut t2d_col) in self
            .get_daq()?
            .axis_iter(Axis(0))
            .skip(start_row)
            .take(frame_num)
            .zip(t2d.axis_iter_mut(Axis(1)))
        {
            for (tc, t) in thermocouples.iter().zip(t2d_col.iter_mut()) {
                *t = daq_row[tc.column_num];
            }
        }

        let regulator = Array::from_shape_vec((regulator.len(), 1), regulator.clone())
            .map_err(|err| awsl!(err))?;

        self.t2d.insert(t2d * regulator);

        Ok(self)
    }

    pub fn interp_single_frame(&mut self, frame: usize) -> TLCResult<Array2<f32>> {
        if self.interp.is_none() {
            self.interp()?;
        }
        self.get_interp()?
            .interp_single_frame(frame, self.config.region_shape)
    }

    /// interpolation of reference temperature matrix
    pub fn interp(&mut self) -> TLCResult<&mut Self> {
        if self.t2d.is_none() {
            self.init_t2d()?;
        }

        let TLCConfig {
            interp_method,
            top_left_pos,
            region_shape,
            ref thermocouples,
            ..
        } = self.config;
        let t2d = self.get_t2d()?;

        let interp = Interp::new(
            t2d,
            interp_method,
            thermocouples,
            top_left_pos,
            region_shape,
        )?;
        self.interp.insert(interp);

        Ok(self)
    }
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

impl Default for InterpMethod {
    fn default() -> Self {
        InterpMethod::Horizontal
    }
}

#[derive(Debug)]
pub struct Interp(Array2<f32>);

impl Interp {
    fn new(
        t2d: ArrayView2<f32>,
        interp_method: InterpMethod,
        thermocouples: &[Thermocouple],
        top_left_pos: (usize, usize),
        region_shape: (usize, usize),
    ) -> TLCResult<Self> {
        match interp_method {
            InterpMethod::Bilinear(_) | InterpMethod::BilinearExtra(_) => Self::interp_bilinear(
                t2d,
                interp_method,
                region_shape,
                thermocouples,
                top_left_pos,
            ),

            _ => Self::interp1d(
                t2d,
                interp_method,
                region_shape,
                thermocouples,
                top_left_pos,
            ),
        }
        .ok_or(awsl!("参考温度插值错误"))
    }

    pub fn interp_single_point(&self, pos: usize, region_shape: (usize, usize)) -> ArrayView1<f32> {
        let (cal_h, cal_w) = region_shape;
        let pos = match self.0.nrows() {
            h if h == cal_w => pos % cal_w,
            h if h == cal_h => pos / cal_w,
            _ => pos,
        };

        self.0.row(pos)
    }

    fn interp_single_frame(
        &self,
        frame: usize,
        region_shape: (usize, usize),
    ) -> TLCResult<Array2<f32>> {
        let (cal_h, cal_w) = region_shape;
        let col = self.0.column(frame);
        let single_frame = match self.0.nrows() {
            h if h == cal_w => col
                .broadcast((cal_h, cal_w))
                .ok_or(awsl!("参考温度矩阵形状转换失败"))?
                .to_owned(),
            h if h == cal_h => col
                .to_owned()
                .into_shape((cal_h, 1))
                .map_err(|err| awsl!(err))?
                .broadcast((cal_h, cal_w))
                .ok_or(awsl!("参考温度矩阵形状转换失败"))?
                .to_owned(),
            _ => col
                .to_owned()
                .into_shape(region_shape)
                .map_err(|err| awsl!(err))?
                .to_owned(),
        };

        let arr: Vec<f32> = single_frame
            .exact_chunks((SCALING, SCALING))
            .into_iter()
            .map(|a| a.mean().unwrap())
            .collect();
        let mut single_frame = Array2::from_shape_vec((cal_h / SCALING, cal_w / SCALING), arr)
            .map_err(|err| awsl!(err))?;
        single_frame.invert_axis(Axis(0));

        Ok(single_frame)
    }

    fn interp1d(
        t2d: ArrayView2<f32>,
        interp_method: InterpMethod,
        region_shape: (usize, usize),
        tcs: &[Thermocouple],
        tl_pos: (usize, usize),
    ) -> Option<Interp> {
        let (cal_h, cal_w) = region_shape;
        let frame_num = t2d.ncols();

        let (interp_len, tc_pos): (_, Vec<_>) = match interp_method {
            InterpMethod::Horizontal | InterpMethod::HorizontalExtra => (
                cal_w,
                tcs.iter().map(|tc| tc.pos.1 - tl_pos.1 as i32).collect(),
            ),
            InterpMethod::Vertical | InterpMethod::VerticalExtra => (
                cal_h,
                tcs.iter().map(|tc| tc.pos.0 - tl_pos.0 as i32).collect(),
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
            .try_for_each(|(mut row, pos)| -> Option<()> {
                let pos = pos as i32;
                let (mut li, mut ri) = (0, 1);
                while pos >= tc_pos[ri] && ri < tc_pos.len() - 1 {
                    li += 1;
                    ri += 1;
                }
                let (l, r) = (tc_pos[li], tc_pos[ri]);
                let (l_temps, r_temps) = (t2d.row(li), t2d.row(ri));
                let l_temps = l_temps.as_slice_memory_order()?;
                let r_temps = r_temps.as_slice_memory_order()?;

                let pos = if do_extra { pos } else { pos.max(l).min(r) };

                let row = row.as_slice_memory_order_mut()?;
                let mut frame = 0;
                while frame + f32x8::lanes() < frame_num {
                    let lv = f32x8::from_slice_unaligned(&l_temps[frame..]);
                    let rv = f32x8::from_slice_unaligned(&r_temps[frame..]);
                    let v8 = (lv * (r - pos) as f32 + rv * (pos - l) as f32) / (r - l) as f32;
                    v8.write_to_slice_unaligned(&mut row[frame..]);
                    frame += f32x8::lanes();
                }
                while frame < frame_num {
                    let (lv, rv) = (l_temps[frame], r_temps[frame]);
                    row[frame] = (lv * (r - pos) as f32 + rv * (pos - l) as f32) / (r - l) as f32;
                    frame += 1;
                }

                Some(())
            })?;

        Some(Interp(temps))
    }

    fn interp_bilinear(
        t2d: ArrayView2<f32>,
        interp_method: InterpMethod,
        region_shape: (usize, usize),
        tcs: &[Thermocouple],
        tl_pos: (usize, usize),
    ) -> Option<Interp> {
        let (tc_shape, do_extra) = match interp_method {
            InterpMethod::Bilinear(tc_shape) => (tc_shape, false),
            InterpMethod::BilinearExtra(tc_shape) => (tc_shape, true),
            _ => unreachable!(),
        };
        let (tc_h, tc_w) = tc_shape;
        let tc_x: Vec<_> = tcs
            .iter()
            .take(tc_w)
            .map(|tc| tc.pos.1 - tl_pos.1 as i32)
            .collect();
        let tc_y: Vec<_> = tcs
            .iter()
            .step_by(tc_w)
            .take(tc_h)
            .map(|tc| tc.pos.0 - tl_pos.0 as i32)
            .collect();

        let (cal_h, cal_w) = region_shape;
        let frame_num = t2d.ncols();
        let pix_num = cal_h * cal_w;
        let mut temps = Array2::zeros((pix_num, frame_num));

        temps
            .axis_iter_mut(Axis(0))
            .into_par_iter()
            .zip(0..pix_num)
            .try_for_each(|(mut row, pos)| -> Option<()> {
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
                let t00 = t00.as_slice_memory_order()?;
                let t01 = t01.as_slice_memory_order()?;
                let t10 = t10.as_slice_memory_order()?;
                let t11 = t11.as_slice_memory_order()?;

                let x = if do_extra { x } else { x.max(x0).min(x1) };
                let y = if do_extra { y } else { y.max(y0).min(y1) };

                let row = row.as_slice_memory_order_mut()?;
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

                Some(())
            })?;

        Some(Interp(temps))
    }
}

#[test]
fn interp_bilinear() -> Result<(), Box<dyn std::error::Error>> {
    let t2d = array![[1.], [2.], [3.], [4.], [5.], [6.]];
    println!("{:?}", t2d.shape());
    let interp_method = InterpMethod::BilinearExtra((2, 3));
    let region_shape = (14, 14);
    let tcs: Vec<Thermocouple> = [(10, 10), (10, 15), (10, 20), (20, 10), (20, 15), (20, 20)]
        .iter()
        .map(|&pos| Thermocouple { column_num: 0, pos })
        .collect();
    let tl_pos = (8, 8);

    let interp =
        Interp::interp_bilinear(t2d.view(), interp_method, region_shape, &tcs, tl_pos).unwrap();

    let res = interp.interp_single_frame(0, region_shape)?;
    println!("{:?}", res);

    Ok(())
}

#[test]
fn interp() -> Result<(), Box<dyn std::error::Error>> {
    const CONFIG_PATH: &str = "./cache/default_config.json";
    let mut tlc_data = TLCData::from_path(CONFIG_PATH).unwrap();
    tlc_data.read_daq()?;
    let t = std::time::Instant::now();
    tlc_data.interp()?;
    println!("{:?}", t.elapsed());
    super::postprocess::plot_line(
        tlc_data
            .get_interp()?
            .interp_single_point(1000, tlc_data.get_config().region_shape),
    )?;

    Ok(())
}
