use std::f32::{consts::PI, NAN};

use libm::erfcf;

use ndarray::prelude::*;

use rayon::prelude::*;

use packed_simd::{f32x8, Simd};

use crate::err;

use super::{error::TLCResult, preprocess::Interp};

const FIRST_FEW_TO_CAL_T0: usize = 4;

/// temporary fake SIMD wrapper of erfcf
fn erfcf_simd(arr: Simd<[f32; 8]>) -> Simd<[f32; 8]> {
    let (x0, x1, x2, x3, x4, x5, x6, x7): (f32, f32, f32, f32, f32, f32, f32, f32) =
        unsafe { std::mem::transmute(arr) };
    f32x8::new(
        erfcf(x0),
        erfcf(x1),
        erfcf(x2),
        erfcf(x3),
        erfcf(x4),
        erfcf(x5),
        erfcf(x6),
        erfcf(x7),
    )
}

/// struct that stores necessary information for solving the equation
struct PointData<'a> {
    peak_frame: usize,
    temps: &'a [f32],
    peak_temp: f32,
    dt: f32,
    h0: f32,
    max_iter_num: usize,
    solid_thermal_conductivity: f32,
    solid_thermal_diffusivity: f32,
}

impl PointData<'_> {
    /// semi-infinite plate heat transfer equation of each pixel(simd)
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self, h: f32) -> (f32, f32) {
        let (k, a, dt, temps, tw, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.temps,
            self.peak_temp,
            self.peak_frame,
        );
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f32>() / FIRST_FEW_TO_CAL_T0 as f32;
        let (mut sum, mut diff_sum) = (f32x8::splat(0.), f32x8::splat(0.));

        let mut i = 1;
        while i + f32x8::lanes() < peak_frame {
            let delta_temp = unsafe {
                f32x8::from_slice_unaligned_unchecked(&temps[i..])
                    - f32x8::from_slice_unaligned_unchecked(&temps[i - 1..])
            };
            let at = a
                * dt
                * f32x8::new(
                    (peak_frame - i) as f32,
                    (peak_frame - i - 1) as f32,
                    (peak_frame - i - 2) as f32,
                    (peak_frame - i - 3) as f32,
                    (peak_frame - i - 4) as f32,
                    (peak_frame - i - 5) as f32,
                    (peak_frame - i - 6) as f32,
                    (peak_frame - i - 7) as f32,
                );
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfcf_simd(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            i += f32x8::lanes();
            sum += step;
            diff_sum += d_step;
        }

        let (mut sum, mut diff_sum) = (sum.sum(), diff_sum.sum());

        while i < peak_frame {
            let delta_temp = unsafe { temps.get_unchecked(i) - temps.get_unchecked(i - 1) };
            let at = a * dt * (peak_frame - i) as f32;
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfcf(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            i += 1;
            sum += step;
            diff_sum += d_step;
        }

        (tw - t0 - sum, diff_sum)
    }

    #[allow(dead_code)]
    fn newton_tangent(&self) -> f32 {
        let mut h = self.h0;

        for _ in 0..self.max_iter_num {
            let (f, df) = self.thermal_equation(h);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                return next_h;
            }
            h = next_h;
        }

        h
    }

    #[allow(dead_code)]
    fn newton_down(&self) -> f32 {
        let mut h = self.h0;
        let (mut f, mut df) = self.thermal_equation(h);

        for _ in 0..self.max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                let (next_f, next_df) = self.thermal_equation(next_h);
                if next_f.abs() < f.abs() {
                    if (next_h - h).abs() < 1e-3 {
                        return next_h;
                    }
                    h = next_h;
                    f = next_f;
                    df = next_df;
                    break;
                }
                lambda /= 2.;
                if lambda < 1e-3 {
                    return NAN;
                }
            }
            if h > 10000. {
                return NAN;
            }
        }

        h
    }
}

pub fn solve<'a>(
    peak_frames: &[usize],
    interp: &Interp,
    solid_thermal_conductivity: f32,
    solid_thermal_diffusivity: f32,
    characteristic_length: f32,
    air_thermal_conductivity: f32,
    dt: f32,
    peak_temp: f32,
    h0: f32,
    max_iter_num: usize,
) -> TLCResult<Array1<f32>> {
    let mut nus = Vec::with_capacity(peak_frames.len());
    unsafe { nus.set_len(peak_frames.len()) };

    peak_frames
        .into_par_iter()
        .enumerate()
        .zip(nus.par_iter_mut())
        .try_for_each(|((pos, &peak_frame), nu)| -> TLCResult<()> {
            *nu = if peak_frame > FIRST_FEW_TO_CAL_T0 {
                let temps = interp.interp_single_point(pos);
                let temps = temps.as_slice_memory_order().ok_or(err!())?;
                let point_data = PointData {
                    peak_frame,
                    temps,
                    peak_temp,
                    dt,
                    h0,
                    max_iter_num,
                    solid_thermal_conductivity,
                    solid_thermal_diffusivity,
                };
                point_data.newton_tangent() * characteristic_length / air_thermal_conductivity
            } else {
                NAN
            };

            Ok(())
        })?;

    Ok(Array1::from(nus))
}
