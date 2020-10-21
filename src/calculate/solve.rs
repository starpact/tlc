#![allow(dead_code)]

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
use std::f64::consts::PI;

fn thermal_equation(
    const_vals: (f64, f64, f64, f64),
    peak_frame: usize,
    t2d: ArrayView1<f64>,
    h: f64,
) -> (f64, f64) {
    let (k, a, dt, tw) = const_vals;
    let t0 = t2d[0];

    let res = t2d
        .iter()
        .skip(1)
        .take(peak_frame - 1)
        .fold((0., 0., 0.), |tmp, &delta_temp| {
            let (f, df, t) = tmp;
            let at = a * t;
            let er = erfc(h * at.sqrt() / k);
            let iter = (1. - f64::exp(h.powf(2.) * at / k.powf(2.)) * er) * delta_temp;
            let d_iter = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt()
                    - (2. * at * h * f64::exp(at * h.powf(2.) / k.powf(2.)) * er) / k.powf(2.));

            (f + iter, df + d_iter, t + dt)
        });

    (tw - t0 - res.0, res.1)
}
