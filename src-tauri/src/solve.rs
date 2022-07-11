use std::{
    f64::{consts::PI, NAN},
    sync::{Arc, RwLock},
};

use libm::erfc;
use packed_simd::{f64x4, Simd};
use serde::{Deserialize, Serialize};

use crate::video::VideoData;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct PhysicalParam {
    peak_temperature: f64,
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum IterationMethod {
    NewtonTangent { h0: f64, max_iter_num: usize },
    NewtonDown { h0: f64, max_iter_num: usize },
}

struct PointData<'a> {
    peak_frame_index: usize,
    temperatures: &'a [f64],
}

trait SolveSinglePoint {
    fn solve_single_point(&self, point_data: PointData) -> f64;
}

struct NewtonTangentSolver {
    physical_param: PhysicalParam,
    max_iter_num: usize,
    h0: f64,
    dt: f64,
}

struct NewtonDownSolver {
    physical_param: PhysicalParam,
    max_iter_num: usize,
    h0: f64,
    dt: f64,
}

impl PointData<'_> {
    fn heat_transfer_equation(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
        let peak_frame_index = self.peak_frame_index;
        let temps = self.temperatures;

        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;
        let (mut sum, mut diff_sum) = (f64x4::default(), f64x4::default());

        let mut frame_index = 0;
        while frame_index + f64x4::lanes() < peak_frame_index {
            let delta_temp = unsafe {
                f64x4::from_slice_unaligned_unchecked(&temps[frame_index + 1..])
                    - f64x4::from_slice_unaligned_unchecked(&temps[frame_index..])
            };
            let at = a
                * dt
                * f64x4::new(
                    (peak_frame_index - frame_index - 1) as f64,
                    (peak_frame_index - frame_index - 2) as f64,
                    (peak_frame_index - frame_index - 3) as f64,
                    (peak_frame_index - frame_index - 4) as f64,
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

        while frame_index < peak_frame_index {
            let delta_temp =
                unsafe { temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index) };
            let at = a * dt * (peak_frame_index - frame_index - 1) as f64;
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

impl SolveSinglePoint for NewtonTangentSolver {
    fn solve_single_point(&self, point_data: PointData) -> f64 {
        let PhysicalParam {
            peak_temperature: tw,
            solid_thermal_conductivity: k,
            solid_thermal_diffusivity: a,
            ..
        } = self.physical_param;

        let mut h = self.h0;
        for _ in 0..self.max_iter_num {
            let (f, df) = point_data.heat_transfer_equation(h, self.dt, k, a, tw);
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
}

impl SolveSinglePoint for NewtonDownSolver {
    fn solve_single_point(&self, point_data: PointData) -> f64 {
        let PhysicalParam {
            peak_temperature: tw,
            solid_thermal_conductivity: k,
            solid_thermal_diffusivity: a,
            ..
        } = self.physical_param;

        let mut h = self.h0;
        let (mut f, mut df) = point_data.heat_transfer_equation(h, self.dt, k, a, tw);
        for _ in 0..self.max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                if (next_h - h).abs() < 1e-3 {
                    return next_h;
                }
                let (next_f, next_df) =
                    point_data.heat_transfer_equation(next_h, self.dt, k, a, tw);
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

        h
    }
}

impl Default for IterationMethod {
    fn default() -> Self {
        Self::NewtonTangent {
            h0: 50.0,
            max_iter_num: 10,
        }
    }
}

pub fn solve(
    video_data: Arc<RwLock<VideoData>>,
    physical_param: PhysicalParam,
    iteration_method: IterationMethod,
    frame_rate: usize,
) {
    rayon::spawn(move || {
        let _green2 = video_data.read().unwrap().green2().unwrap();
        let dt = 1.0 / frame_rate as f64;
        match iteration_method {
            IterationMethod::NewtonTangent { h0, max_iter_num } => {
                solve_core(NewtonTangentSolver {
                    physical_param,
                    max_iter_num,
                    dt,
                    h0,
                })
            }
            IterationMethod::NewtonDown { h0, max_iter_num } => solve_core(NewtonDownSolver {
                physical_param,
                max_iter_num,
                dt,
                h0,
            }),
        }
    });

    todo!()
}

fn solve_core<S: SolveSinglePoint>(_solver: S) {
    todo!()
}

#[cfg(test)]
mod tests {
    extern crate test;
    use crate::daq::DaqDataManager;

    use super::*;
    use approx::assert_relative_eq;
    use ndarray::Array1;
    use tauri::async_runtime;
    use test::Bencher;

    impl PointData<'_> {
        fn iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
            let peak_frame_index = self.peak_frame_index;
            let temperatures = self.temperatures;
            // We use the average of first 4 values to calculate the initial temperature.
            const FIRST_FEW_TO_CAL_T0: usize = 4;
            let t0 = temperatures[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>()
                / FIRST_FEW_TO_CAL_T0 as f64;

            let (sum, diff_sum) = temperatures
                .array_windows::<2>()
                .take(peak_frame_index)
                .enumerate()
                .fold((0.0, 0.0), |(sum, diff_sum), (frame_index, temps_2)| {
                    let delta_temp = unsafe { temps_2.get_unchecked(1) - temps_2.get_unchecked(0) };
                    let at = a * dt * (peak_frame_index - frame_index - 1) as f64;
                    let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());

                    let step = (1.0 - exp_erfc) * delta_temp;
                    let diff_step = -delta_temp
                        * (2.0 * at.sqrt() / k / PI.sqrt()
                            - (2.0 * at * h * exp_erfc) / k.powf(2.));

                    (sum + step, diff_sum + diff_step)
                });

            (tw - t0 - sum, diff_sum)
        }

        fn no_iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
            let peak_frame_index = self.peak_frame_index;
            let temps = self.temperatures;
            // We use the average of first 4 values to calculate the initial temperature.
            const FIRST_FEW_TO_CAL_T0: usize = 4;
            let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;

            let (mut sum, mut diff_sum) = (0., 0.);
            for frame_index in 0..peak_frame_index {
                let delta_temp = unsafe {
                    temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index)
                };
                let at = a * dt * (peak_frame_index - frame_index - 1) as f64;
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
        async_runtime::block_on(async {
            DaqDataManager::default()
                .read_daq("/home/yhj/Documents/2021yhj/EXP/imp/daq/imp_20000_1.lvm")
                .await
                .unwrap()
                .column(3)
                .to_owned()
        })
    }

    const I: (f64, f64, f64, f64, f64) = (100.0, 0.04, 0.19, 1.091e-7, 35.48);

    #[test]
    fn test_result_correct() {
        let temps = new_temps();
        let point_data = PointData {
            peak_frame_index: 800,
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
            peak_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 8,658 ns/iter (+/- 90)
        b.iter(|| point_data.heat_transfer_equation(I.0, I.1, I.2, I.3, I.4));
    }

    #[bench]
    fn bench_iter_no_simd(b: &mut Bencher) {
        let temps = new_temps();
        let point_data = PointData {
            peak_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 13,194 ns/iter (+/- 194)
        b.iter(|| point_data.iter_no_simd(I.0, I.1, I.2, I.3, I.4));
    }

    #[bench]
    fn bench_no_iter_no_simd(b: &mut Bencher) {
        let temps = new_temps();
        let point_data = PointData {
            peak_frame_index: 800,
            temperatures: temps.as_slice_memory_order().unwrap(),
        };

        // 13,198 ns/iter (+/- 175)
        b.iter(|| point_data.no_iter_no_simd(I.0, I.1, I.2, I.3, I.4));
    }
}
