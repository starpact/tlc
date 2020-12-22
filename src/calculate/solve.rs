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
struct SinglePointSolver<'a> {
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
    dt: f64,
    peak_frame: usize,
    peak_temp: f64,
    temps: ArrayView1<'a, f64>,
    h: f64,
    max_iter_num: usize,
}

impl SinglePointSolver<'_> {
    /// *semi-infinite plate heat transfer equation of each pixel*
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self) -> (f64, f64) {
        let (k, a, dt, tw, h, temps, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.peak_temp,
            self.h,
            self.temps,
            self.peak_frame,
        );

        let res = (1..peak_frame).fold((0., 0.), |(f, df), i| {
            let delta_temp = unsafe { temps.uget(i) - temps.uget(i - 1) };
            let at = a * dt * (peak_frame - i) as f64;
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfc(h / k * at.sqrt());
            let iter = (1. - exp_erfc) * delta_temp;
            let d_iter = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            (f + iter, df + d_iter)
        });

        let t0 = self.temps.slice(s![..4]).mean().unwrap();

        (tw - t0 - res.0, res.1)
    }

    fn newton_tangent(&mut self) -> f64 {
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

struct DoublePointeSolver<'a> {
    exp1: SinglePointSolver<'a>,
    exp2: SinglePointSolver<'a>,
}

pub fn solve(
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
    dt: f64,
    peak_temp: f64,
    peak_frames: ArrayView1<usize>,
    interp_temps: ArrayView2<f64>,
    query_index: ArrayView1<usize>,
    h0: f64,
    max_iter_num: usize,
) -> Array1<f64> {
    let mut nus = Array1::zeros(query_index.len());

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
                temps: interp_temps.column(index),
                h: h0,
                max_iter_num,
            };
            *nu = single_point_solver.newton_tangent();
        });

    nus
}
