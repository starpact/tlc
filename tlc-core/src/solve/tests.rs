extern crate test;

use std::simd::SimdFloat;

use approx::assert_relative_eq;
use sleef::Sleef;
use test::Bencher;

use crate::daq;

use super::*;

impl PointData<'_> {
    fn heat_transfer_equation_iter_no_simd(
        &self,
        h: f64,
        dt: f64,
        k: f64,
        a: f64,
        tw: f64,
    ) -> (f64, f64) {
        let gmax_frame_index = self.gmax_frame_index;
        let temperatures = self.temperatures;
        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 =
            temperatures[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;

        let (sum, diff_sum) = temperatures
            .array_windows()
            .take(gmax_frame_index)
            .enumerate()
            .fold(
                (0.0, 0.0),
                |(sum, diff_sum), (frame_index, [temp1, temp2])| {
                    let delta_temp = temp2 - temp1;
                    let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
                    let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());

                    let step = (1.0 - exp_erfc) * delta_temp;
                    let diff_step = -delta_temp
                        * (2.0 * at.sqrt() / k / PI.sqrt()
                            - (2.0 * at * h * exp_erfc) / k.powf(2.));

                    (sum + step, diff_sum + diff_step)
                },
            );

        (tw - t0 - sum, diff_sum)
    }

    fn heat_transfer_equation_no_iter_no_simd(
        &self,
        h: f64,
        dt: f64,
        k: f64,
        a: f64,
        tw: f64,
    ) -> (f64, f64) {
        let gmax_frame_index = self.gmax_frame_index;
        let temps = self.temperatures;
        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;

        let (mut sum, mut diff_sum) = (0., 0.);
        for frame_index in 0..gmax_frame_index {
            let delta_temp =
                unsafe { temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index) };
            let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());

            let step = (1.0 - exp_erfc) * delta_temp;
            let diff_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.));

            sum += step;
            diff_sum += diff_step;
        }

        (tw - t0 - sum, diff_sum)
    }

    fn heat_transfer_equation_simd_sleef(
        self,
        h: f64,
        dt: f64,
        k: f64,
        a: f64,
        tw: f64,
    ) -> (f64, f64) {
        use core::simd::f64x4;

        let gmax_frame_index = self.gmax_frame_index;
        let temps = self.temperatures;

        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;
        let (mut sum, mut diff_sum) = (f64x4::default(), f64x4::default());

        let mut frame_index = 0;
        {
            let h = f64x4::splat(h);
            let k = f64x4::splat(k);
            let one = f64x4::splat(1.0);
            let two = f64x4::splat(2.0);
            let pi_sqrt = f64x4::splat(PI.sqrt());
            while frame_index + 4 < gmax_frame_index {
                let delta_temp = f64x4::from_array(unsafe {
                    [
                        *temps.get_unchecked(frame_index + 1),
                        *temps.get_unchecked(frame_index + 2),
                        *temps.get_unchecked(frame_index + 3),
                        *temps.get_unchecked(frame_index + 4),
                    ]
                }) - f64x4::from_array(unsafe {
                    [
                        *temps.get_unchecked(frame_index),
                        *temps.get_unchecked(frame_index + 1),
                        *temps.get_unchecked(frame_index + 2),
                        *temps.get_unchecked(frame_index + 3),
                    ]
                });
                let at = f64x4::splat(a * dt)
                    * f64x4::from_array([
                        (gmax_frame_index - frame_index - 1) as f64,
                        (gmax_frame_index - frame_index - 2) as f64,
                        (gmax_frame_index - frame_index - 3) as f64,
                        (gmax_frame_index - frame_index - 4) as f64,
                    ]);
                let exp_erfc = (h.pow(two) / k.pow(two) * at).exp() * (h / k * at.sqrt()).erfc();
                let step = (one - exp_erfc) * delta_temp;
                let diff_step = -delta_temp
                    * (two * at.sqrt() / k / pi_sqrt - two * at * h * exp_erfc / k.pow(two));

                sum += step;
                diff_sum += diff_step;
                frame_index += 4;
            }
        }

        let (mut sum, mut diff_sum) = (sum.reduce_sum(), diff_sum.reduce_sum());

        while frame_index < gmax_frame_index {
            let delta_temp =
                unsafe { temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index) };
            let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());
            let step = (1.0 - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.0));

            sum += step;
            diff_sum += d_step;
            frame_index += 1;
        }

        (tw - t0 - sum, diff_sum)
    }
}

fn new_temps() -> Vec<f64> {
    let daq_data = daq::io::read_daq("./testdata/imp_20000_1.lvm").unwrap();
    daq_data.column(3).to_vec()
}

const H0: f64 = 100.0;
const DT: f64 = 0.04;
const K: f64 = 0.19;
const A: f64 = 1.091e-7;
const TW: f64 = 35.48;

#[test]
fn test_single_point_correct() {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    let r1 = point_data.heat_transfer_equation(H0, DT, K, A, TW);
    let r2 = point_data.heat_transfer_equation_iter_no_simd(H0, DT, K, A, TW);
    let r3 = point_data.heat_transfer_equation_no_iter_no_simd(H0, DT, K, A, TW);
    let r4 = point_data.heat_transfer_equation_simd_sleef(H0, DT, K, A, TW);
    println!(
        "simd:\t\t\t{r1:?}\niter_no_simd:\t\t{r2:?}\nno_iter_no_simd:\t{r3:?}\nsleef:\t{r4:?}"
    );

    assert_relative_eq!(r1.0, r2.0, max_relative = 1e-6);
    assert_relative_eq!(r1.1, r2.1, max_relative = 1e-6);
    assert_relative_eq!(r1.0, r3.0, max_relative = 1e-6);
    assert_relative_eq!(r1.1, r3.1, max_relative = 1e-6);
    assert_relative_eq!(r1.0, r4.0, max_relative = 1e-6);
    assert_relative_eq!(r1.1, r4.1, max_relative = 1e-6);
}

// Bench on 13th Gen Intel i9-13900K (32) @ 5.500GHz.
//
// 1. SIMD is about 40% faster than scalar version. Considering that we do not
// find vectorized erfc implementation for rust, this is acceptable.
// 2. Generally iteration has the same performance as for loop. No auto-vectorization
// in this case.

#[bench]
fn bench_simd(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 4,589 ns/iter (+/- 66)
    b.iter(|| point_data.heat_transfer_equation(H0, DT, K, A, TW));
}

#[bench]
fn bench_simd_sleef(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 29,497 ns/iter (+/- 48)
    b.iter(|| point_data.heat_transfer_equation_simd_sleef(H0, DT, K, A, TW));
}

#[bench]
fn bench_iter_no_simd(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 6,337 ns/iter (+/- 15)
    b.iter(|| point_data.heat_transfer_equation_iter_no_simd(H0, DT, K, A, TW));
}

#[bench]
fn bench_no_iter_no_simd(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 6,295 ns/iter (+/- 55)
    b.iter(|| point_data.heat_transfer_equation_no_iter_no_simd(H0, DT, K, A, TW));
}
