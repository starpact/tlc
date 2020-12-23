use ndarray::prelude::*;
use ndarray::Zip;
use std::f64::{consts::PI, NAN};

/// there is no erfc() in std, so use erfc() from libc
mod cmath {
    use libc::c_double;
    extern "C" {
        pub fn erfc(x: c_double) -> c_double;
    }
}

fn erfc(x: f64) -> f64 {
    unsafe { cmath::erfc(x) }
}

/// *struct that stores necessary information for solving the equation*
struct PointData<'a> {
    peak_frame: usize,
    temps: ArrayView1<'a, f64>,
    peak_temp: f64,
    dt: f64,
    h0: f64,
    max_iter_num: usize,
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

impl PointData<'_> {
    /// *semi-infinite plate heat transfer equation of each pixel*
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self, h: f64) -> (f64, f64) {
        let (k, a, dt, temps, tw, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.temps,
            self.peak_temp,
            self.peak_frame,
        );

        let (sum, diff_sum) = (1..peak_frame).fold((0., 0.), |(f, df), i| {
            let delta_temp = unsafe { temps.uget(i) - temps.uget(i - 1) };
            let at = a * dt * (peak_frame - i) as f64;
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfc(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            (f + step, df + d_step)
        });

        let t0 = self.temps.slice(s![..4]).mean().unwrap();

        (tw - t0 - sum, diff_sum)
    }

    #[allow(dead_code)]
    fn newton_tangent(&self) -> f64 {
        let mut h = self.h0;
        for _ in 0..self.max_iter_num {
            let (f, df) = self.thermal_equation(h);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                break;
            }
            h = next_h;
        }

        h * self.characteristic_length / self.air_thermal_conductivity
    }

    #[allow(dead_code)]
    fn newton_down(&self) -> f64 {
        let mut h = self.h0;
        let (mut f, mut df) = self.thermal_equation(h);

        for _ in 0..self.max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                let (next_f, next_df) = self.thermal_equation(next_h);
                if next_f.abs() < f.abs() {
                    if (next_h - h).abs() < 1e-3 {
                        break;
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

        h * self.characteristic_length / self.air_thermal_conductivity
    }
}

pub fn solve(
    peak_frames: ArrayView1<usize>,
    interp_temps: ArrayView2<f64>,
    query_index: ArrayView1<usize>,
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
    dt: f64,
    peak_temp: f64,
    h0: f64,
    max_iter_num: usize,
) -> Array1<f64> {
    let mut nus = Array1::zeros(query_index.len());

    Zip::from(&mut nus)
        .and(query_index)
        .and(peak_frames)
        .par_apply(|nu, &index, &peak_frame| {
            let point_data = PointData {
                peak_frame,
                temps: interp_temps.column(index),
                peak_temp,
                dt,
                h0,
                max_iter_num,
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity,
            };
            *nu = point_data.newton_tangent();
        });

    nus
}
