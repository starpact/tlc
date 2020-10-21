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
use ndarray::Zip;
use std::f64::consts::PI;

#[inline(always)]
fn thermal_equation(
    const_vals: (f64, f64, f64, f64),
    peak_frame: usize,
    delta_temps: ArrayView1<f64>,
    h: f64,
) -> (f64, f64) {
    let (k, a, dt, tw) = const_vals;
    let t0 = delta_temps[0];

    let res =
        delta_temps
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

#[inline(always)]
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

pub fn solve(
    const_vals: (f64, f64, f64, f64),
    peak_frames: Array1<usize>,
    delta_temps_2d: Array2<f64>,
    h0: f64,
    max_iter_num: usize,
) -> Array1<f64> {
    let (pix_num, cal_w) = (peak_frames.len(), delta_temps_2d.ncols());
    let cal_h = pix_num / cal_w;
    let mut hs = Array1::zeros(pix_num);
    let mut xs = Array1::zeros(pix_num);
    let mut iter = xs.iter_mut();
    for _ in 0..cal_h {
        for i in 0..cal_w {
            *iter.next().unwrap() = i;
        }
    }

    Zip::from(&peak_frames).and(&xs).and(&mut hs).par_apply(|&peak_frame, &x, h| {
        *h = newtow_tangent(const_vals, peak_frame, delta_temps_2d.column(x), h0, max_iter_num);
    });

    hs
}
