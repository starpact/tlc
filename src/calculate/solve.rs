#![allow(dead_code)]

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

/// *semi-infinite plate heat transfer equation of each pixel*
/// ### Argument:
/// (conductivity, diffusivity, time_step, the peak temperature(wall temp at peak frame))
///
/// the frame that reaches the max green value
///
/// delta temperature history of this pixel(initial values in the first row)
///
/// heat transfer coefficient(a certain value during iterating)
/// ### Return:
/// equation and its derivative
fn thermal_equation(
    const_vals: (f64, f64, f64, f64),
    peak_frame: usize,
    delta_temps: ArrayView1<f64>,
    h: f64,
) -> (f64, f64) {
    let (k, a, dt, tw) = const_vals;
    let t0 = delta_temps[0];

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

fn newtow_tangent(
    const_vals: (f64, f64, f64, f64),
    peak_frame: usize,
    delta_temps: ArrayView1<f64>,
    h0: f64,
    max_iter_num: usize,
) -> f64 {
    let mut h = h0;
    for _ in 0..max_iter_num {
        let (f, df) = thermal_equation(const_vals, peak_frame, delta_temps, h);
        let hh = h - f / df;
        if hh.abs() > 10000. {
            return std::f64::NAN;
        }
        if (hh - h).abs() < 1e-3 {
            break;
        }
        h = hh;
    }

    h
}

/// *calculate the delta temperature of adjacent frames for the convenience of calculating*
/// *thermal equation, and store the initial value in first row, like:*
///
/// `t0(average), t1 - t0, t2 - t1, ... tn - tn_1`
pub fn cal_delta_temps(mut t2d: Array2<f64>) -> Array2<f64> {
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
    const_vals: (f64, f64, f64, f64),
    peak_frames: Array1<usize>,
    interp_t2d: Array2<f64>,
    query_index: Array1<usize>,
    h0: f64,
    max_iter_num: usize,
) -> Array1<f64> {
    let mut hs = Array1::zeros(query_index.len());
    let delta_temps_2d = cal_delta_temps(interp_t2d);

    Zip::from(&peak_frames)
        .and(&query_index)
        .and(&mut hs)
        .par_apply(|&peak_frame, &index, h| {
            *h = newtow_tangent(
                const_vals,
                peak_frame,
                delta_temps_2d.column(index),
                h0,
                max_iter_num,
            );
        });

    hs
}
