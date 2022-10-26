use std::{
    f64::{consts::PI, NAN},
    sync::Arc,
};

use libm::erfc;
use ndarray::{Array2, ArrayView, Dimension};
use packed_simd::{f64x4, Simd};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::daq::Interpolator;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[cfg_attr(test, derive(Default))]
pub struct PhysicalParam {
    pub gmax_temperature: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum IterationMethod {
    NewtonTangent { h0: f64, max_iter_num: usize },
    NewtonDown { h0: f64, max_iter_num: usize },
}

struct PointData<'a> {
    gmax_frame_index: usize,
    temperatures: &'a [f64],
}

impl PointData<'_> {
    fn heat_transfer_equation(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
        let gmax_frame_index = self.gmax_frame_index;
        let temps = self.temperatures;

        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;
        let (mut sum, mut diff_sum) = (f64x4::default(), f64x4::default());

        let mut frame_index = 0;
        while frame_index + f64x4::lanes() < gmax_frame_index {
            let delta_temp = unsafe {
                f64x4::from_slice_unaligned_unchecked(&temps[frame_index + 1..])
                    - f64x4::from_slice_unaligned_unchecked(&temps[frame_index..])
            };
            let at = a
                * dt
                * f64x4::new(
                    (gmax_frame_index - frame_index - 1) as f64,
                    (gmax_frame_index - frame_index - 2) as f64,
                    (gmax_frame_index - frame_index - 3) as f64,
                    (gmax_frame_index - frame_index - 4) as f64,
                );
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc_simd(h / k * at.sqrt());
            let step = (1.0 - exp_erfc) * delta_temp;
            let diff_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.0));

            frame_index += f64x4::lanes();
            sum += step;
            diff_sum += diff_step;
        }

        let (mut sum, mut diff_sum) = (sum.sum(), diff_sum.sum());

        while frame_index < gmax_frame_index {
            let delta_temp =
                unsafe { temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index) };
            let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());
            let step = (1.0 - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.0));

            frame_index += 1;
            sum += step;
            diff_sum += d_step;
        }

        (tw - t0 - sum, diff_sum)
    }
}

// Fake SIMD version erfc.
// I didn't find rust version SIMD erfc. Maybe use `sleef` binding in the future.
fn erfc_simd(arr: Simd<[f64; 4]>) -> Simd<[f64; 4]> {
    unsafe {
        f64x4::new(
            erfc(arr.extract_unchecked(0)),
            erfc(arr.extract_unchecked(1)),
            erfc(arr.extract_unchecked(2)),
            erfc(arr.extract_unchecked(3)),
        )
    }
}

fn newtow_tangent(
    physical_param: PhysicalParam,
    dt: f64,
    h0: f64,
    max_iter_num: usize,
) -> impl Fn(PointData) -> f64 {
    move |point_data| {
        let PhysicalParam {
            gmax_temperature: tw,
            solid_thermal_conductivity: k,
            solid_thermal_diffusivity: a,
            characteristic_length,
            air_thermal_conductivity,
        } = physical_param;

        let mut h = h0;
        for _ in 0..max_iter_num {
            let (f, df) = point_data.heat_transfer_equation(h, dt, k, a, tw);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                return next_h;
            }
            h = next_h;
        }

        h * characteristic_length / air_thermal_conductivity
    }
}

fn newtow_down(
    physical_param: PhysicalParam,
    dt: f64,
    h0: f64,
    max_iter_num: usize,
) -> impl Fn(PointData) -> f64 {
    move |point_data| {
        let PhysicalParam {
            gmax_temperature: tw,
            solid_thermal_conductivity: k,
            solid_thermal_diffusivity: a,
            characteristic_length,
            air_thermal_conductivity,
        } = physical_param;

        let mut h = h0;
        let (mut f, mut df) = point_data.heat_transfer_equation(h, dt, k, a, tw);
        for _ in 0..max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                if (next_h - h).abs() < 1e-3 {
                    return next_h;
                }
                let (next_f, next_df) = point_data.heat_transfer_equation(next_h, dt, k, a, tw);
                if next_f.abs() < f.abs() {
                    h = next_h;
                    f = next_f;
                    df = next_df;
                    break;
                }
                lambda /= 2.0;
                if lambda < 1e-3 {
                    return NAN;
                }
            }
            if h.abs() > 10000.0 {
                return NAN;
            }
        }

        h * characteristic_length / air_thermal_conductivity
    }
}

impl Default for IterationMethod {
    fn default() -> IterationMethod {
        IterationMethod::NewtonTangent {
            h0: 50.0,
            max_iter_num: 10,
        }
    }
}

#[instrument(skip(gmax_frame_indexes, interpolator))]
pub fn solve(
    gmax_frame_indexes: Arc<Vec<usize>>,
    interpolator: Interpolator,
    physical_param: PhysicalParam,
    iteration_method: IterationMethod,
    frame_rate: usize,
) -> Array2<f64> {
    let dt = 1.0 / frame_rate as f64;
    let shape = interpolator.shape();
    let nu1 = match iteration_method {
        IterationMethod::NewtonTangent { h0, max_iter_num } => solve_inner(
            gmax_frame_indexes,
            interpolator,
            newtow_tangent(physical_param, dt, h0, max_iter_num),
        ),
        IterationMethod::NewtonDown { h0, max_iter_num } => solve_inner(
            gmax_frame_indexes,
            interpolator,
            newtow_down(physical_param, dt, h0, max_iter_num),
        ),
    };

    Array2::from_shape_vec(shape, nu1).unwrap()
}

pub fn nan_mean<D: Dimension>(data: ArrayView<f64, D>) -> f64 {
    let (sum, non_nan_cnt, cnt) = data.iter().fold((0., 0, 0), |(sum, non_nan_cnt, cnt), &x| {
        if x.is_nan() {
            (sum, non_nan_cnt, cnt + 1)
        } else {
            (sum + x, non_nan_cnt + 1, cnt + 1)
        }
    });

    let nan_ratio = (cnt - non_nan_cnt) as f64 / cnt as f64;
    debug!(non_nan_cnt, cnt, nan_ratio);

    sum / non_nan_cnt as f64
}

fn solve_inner<F>(
    gmax_frame_indexes: Arc<Vec<usize>>,
    interpolator: Interpolator,
    solve_single_point: F,
) -> Vec<f64>
where
    F: Fn(PointData) -> f64 + Send + Sync + 'static,
{
    const FIRST_FEW_TO_CAL_T0: usize = 4;
    gmax_frame_indexes
        .par_iter()
        .enumerate()
        .map(|(point_index, &gmax_frame_index)| {
            if gmax_frame_index <= FIRST_FEW_TO_CAL_T0 {
                return NAN;
            }
            let temperatures = interpolator.interp_single_point(point_index);
            let temperatures = temperatures.as_slice().unwrap();
            let point_data = PointData {
                gmax_frame_index,
                temperatures,
            };
            solve_single_point(point_data)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    extern crate test;
    use approx::assert_relative_eq;
    use ndarray::Array1;
    use test::Bencher;

    use super::*;
    use crate::daq;

    impl PointData<'_> {
        fn iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
            let gmax_frame_index = self.gmax_frame_index;
            let temperatures = self.temperatures;
            // We use the average of first 4 values to calculate the initial temperature.
            const FIRST_FEW_TO_CAL_T0: usize = 4;
            let t0 = temperatures[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>()
                / FIRST_FEW_TO_CAL_T0 as f64;

            let (sum, diff_sum) = temperatures
                .array_windows()
                .take(gmax_frame_index)
                .enumerate()
                .fold(
                    (0.0, 0.0),
                    |(sum, diff_sum), (frame_index, [temp1, temp2])| {
                        let delta_temp = temp2 - temp1;
                        let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
                        let exp_erfc =
                            (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());

                        let step = (1.0 - exp_erfc) * delta_temp;
                        let diff_step = -delta_temp
                            * (2.0 * at.sqrt() / k / PI.sqrt()
                                - (2.0 * at * h * exp_erfc) / k.powf(2.));

                        (sum + step, diff_sum + diff_step)
                    },
                );

            (tw - t0 - sum, diff_sum)
        }

        fn no_iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
            let gmax_frame_index = self.gmax_frame_index;
            let temps = self.temperatures;
            // We use the average of first 4 values to calculate the initial temperature.
            const FIRST_FEW_TO_CAL_T0: usize = 4;
            let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;

            let (mut sum, mut diff_sum) = (0., 0.);
            for frame_index in 0..gmax_frame_index {
                let delta_temp = unsafe {
                    temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index)
                };
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
    }

    fn new_temps() -> Array1<f64> {
        let daq_raw = daq::read_daq("./tests/imp_20000_1.lvm").unwrap().1;
        daq_raw.column(3).to_owned()
    }

    const I: (f64, f64, f64, f64, f64) = (100.0, 0.04, 0.19, 1.091e-7, 35.48);

    #[test]
    fn test_single_point_correct() {
        let temps = new_temps();
        let point_data = PointData {
            gmax_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };
        let r1 = point_data.heat_transfer_equation(I.0, I.1, I.2, I.3, I.4);
        let r2 = point_data.iter_no_simd(I.0, I.1, I.2, I.3, I.4);
        let r3 = point_data.no_iter_no_simd(I.0, I.1, I.2, I.3, I.4);
        println!(
            "simd:\t\t\t{:?}\niter_no_simd:\t\t{:?}\nno_iter_no_simd:\t{:?}",
            r1, r2, r3
        );

        assert_relative_eq!(r1.0, r2.0, max_relative = 1e-6);
        assert_relative_eq!(r1.1, r2.1, max_relative = 1e-6);
        assert_relative_eq!(r1.0, r3.0, max_relative = 1e-6);
        assert_relative_eq!(r1.1, r3.1, max_relative = 1e-6);
    }

    // Bench on AMD Ryzen 7 4800H with Radeon Graphics (16) @ 2.900GHz.
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
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 8,658 ns/iter (+/- 90)
        b.iter(|| point_data.heat_transfer_equation(I.0, I.1, I.2, I.3, I.4));
    }

    #[bench]
    fn bench_iter_no_simd(b: &mut Bencher) {
        let temps = new_temps();
        let point_data = PointData {
            gmax_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 13,194 ns/iter (+/- 194)
        b.iter(|| point_data.iter_no_simd(I.0, I.1, I.2, I.3, I.4));
    }

    #[bench]
    fn bench_no_iter_no_simd(b: &mut Bencher) {
        let temps = new_temps();
        let point_data = PointData {
            gmax_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 13,198 ns/iter (+/- 175)
        b.iter(|| point_data.no_iter_no_simd(I.0, I.1, I.2, I.3, I.4));
    }
}
