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
struct PointData<'a> {
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    dt: f64,
    peak_frame: usize,
    temps: ArrayView1<'a, f64>,
}

impl PointData<'_> {
    /// *semi-infinite plate heat transfer equation of each pixel*
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self, h: f64) -> (f64, f64) {
        let (k, a, dt, temps, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.temps,
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

        (t0 + sum, diff_sum)
    }
}

struct SingleSolver<'a> {
    data: PointData<'a>,
    h0: f64,
    peak_temp: f64,
    max_iter_num: usize,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

impl SingleSolver<'_> {
    fn newton_tangent(&self) -> f64 {
        let mut h = self.h0;
        for _ in 0..self.max_iter_num {
            let (sum, diff_sum) = self.data.thermal_equation(h);
            let (f, df) = (self.peak_temp - sum, diff_sum);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return std::f64::NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                break;
            }
            h = next_h;
        }

        h * self.characteristic_length / self.air_thermal_conductivity
    }
}

struct DoubleSolver<'a> {
    data1: PointData<'a>,
    data2: PointData<'a>,
    h0: f64,
    max_iter_num: usize,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

impl DoubleSolver<'_> {
    fn newton_tangent(&self) -> f64 {
        let mut h = self.h0;
        for _ in 0..self.max_iter_num {
            let (sum1, diff_sum1) = self.data1.thermal_equation(h);
            let (sum2, diff_sum2) = self.data2.thermal_equation(h);
            let (f, df) = (sum1 - sum2, diff_sum1 - diff_sum2);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return std::f64::NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                break;
            }
            h = next_h;
        }

        h * self.characteristic_length / self.air_thermal_conductivity
    }
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
            let data = PointData {
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                dt,
                peak_frame,
                temps: interp_temps.column(index),
            };
            let single_solver = SingleSolver {
                data,
                h0,
                peak_temp,
                max_iter_num,
                characteristic_length,
                air_thermal_conductivity,
            };
            *nu = single_solver.newton_tangent();
        });

    nus
}
