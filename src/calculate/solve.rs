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

use ndarray::prelude::*;
use ndarray::Zip;
use std::f64::consts::PI;

/// *struct that stores necessary information for solving the equation*
/// * conductivity
/// * diffusivity
/// * time step
/// * the frame that reaches the max green value
/// * wall temperature at peak frame
/// * delta temperature history of this pixel(initial values in the first row)
/// * heat transfer coefficient(a certain value during iterating)
/// * max step number of iteration
pub struct SinglePointSolver<'a> {
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
    dt: f64,
    peak_frame: usize,
    peak_temp: f64,
    delta_temps: ArrayView1<'a, f64>,
    h: f64,
    max_iter_num: usize,
}

impl SinglePointSolver<'_> {
    /// *semi-infinite plate heat transfer equation of each pixel*
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self) -> (f64, f64) {
        let (k, a, dt, tw, h, delta_temps, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.peak_temp,
            self.h,
            self.delta_temps,
            self.peak_frame,
        );
        let t0 = self.delta_temps[0];

        let res = delta_temps.iter().skip(1).take(peak_frame - 1).fold(
            (0., 0., (peak_frame - 1) as f64 * dt),
            |tmp, &delta_temp| {
                let (f, df, t) = tmp;
                let at = a * t;
                let er = erfc(h * at.sqrt() / k);
                let iter = (1. - f64::exp(h.powf(2.) * at / k.powf(2.)) * er) * delta_temp;
                let d_iter = -delta_temp
                    * (2. * at.sqrt() / k / PI.sqrt()
                        - (2. * at * h * f64::exp(at * h.powf(2.) / k.powf(2.)) * er) / k.powf(2.));

                (f + iter, df + d_iter, t - dt)
            },
        );

        (tw - t0 - res.0, res.1)
    }

    fn newtow_tangent(&mut self) -> f64 {
        for _ in 0..self.max_iter_num {
            let prev_h = self.h;
            let (f, df) = self.thermal_equation();
            self.h = prev_h - f / df;
            if self.h.abs() > 10000. {
                return std::f64::NAN;
            }
            if (self.h - prev_h).abs() < 1e-3 {
                break;
            }
        }

        self.h * self.characteristic_length / self.air_thermal_conductivity
    }
}

/// *calculate the delta temperature of adjacent frames for the convenience of calculating*
/// *thermal equation, and store the initial value in first row, like:*
///
/// `t0(average), t1 - t0, t2 - t1, ... tn - tn_1`
fn cal_delta_temps(mut t2d: Array2<f64>) -> Array2<f64> {
    for mut col in t2d.axis_iter_mut(Axis(1)) {
        if let Some(t0) = col.slice(s![..4]).mean() {
            col[0] = t0;
        }
        col.iter_mut().fold(0., |prev, curr| {
            let tmp = *curr;
            *curr -= prev;
            tmp
        });
    }

    t2d
}

pub fn solve(
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
    dt: f64,
    peak_temp: f64,
    peak_frames: Array1<usize>,
    interp_temps: Array2<f64>,
    query_index: Array1<usize>,
    h0: f64,
    max_iter_num: usize,
) -> Array1<f64> {
    let mut nus = Array1::zeros(query_index.len());
    let delta_temps_2d = cal_delta_temps(interp_temps);

    Zip::from(&peak_frames)
        .and(&query_index)
        .and(&mut nus)
        .par_apply(|&peak_frame, &index, nu| {
            let mut single_point_solver = SinglePointSolver {
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity,
                dt,
                peak_temp,
                peak_frame,
                delta_temps: delta_temps_2d.column(index),
                h: h0,
                max_iter_num,
            };
            *nu = single_point_solver.newtow_tangent();
        });

    nus
}
