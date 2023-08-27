use std::f64::{consts::PI, NAN};

use libm::erfc;
use ndarray::Array2;
use packed_simd::{f64x4, Simd};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::daq::Interpolator;

/// All fields not NAN.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct PhysicalParam {
    pub gmax_temperature: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
}

/// All fields not NAN.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum IterMethod {
    NewtonTangent { h0: f64, max_iter_num: usize },
    NewtonDown { h0: f64, max_iter_num: usize },
}

#[derive(Clone, Copy)]
struct PointData<'a> {
    gmax_frame_index: usize,
    temperatures: &'a [f64],
}

impl PointData<'_> {
    fn heat_transfer_equation(self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
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

// Scalar version erfc from libm is much faster than SIMD version from sleef.
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

fn newtow_tangent<EQ>(equation: EQ, h0: f64, max_iter_num: usize) -> impl Fn(PointData) -> f64
where
    EQ: Fn(PointData, f64) -> (f64, f64),
{
    move |point_data| {
        let mut h = h0;
        for _ in 0..max_iter_num {
            let (f, df) = equation(point_data, h);
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

fn newtow_down<EQ>(equation: EQ, h0: f64, max_iter_num: usize) -> impl Fn(PointData) -> f64
where
    EQ: Fn(PointData, f64) -> (f64, f64),
{
    move |point_data| {
        let mut h = h0;
        let (mut f, mut df) = equation(point_data, h);
        for _ in 0..max_iter_num {
            let mut lambda = 1.0;
            loop {
                let next_h = h - lambda * f / df;
                if (next_h - h).abs() < 1e-3 {
                    return next_h;
                }
                let (next_f, next_df) = equation(point_data, next_h);
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

#[instrument(skip(gmax_frame_indexes, interpolator))]
pub fn solve_nu(
    frame_rate: usize,
    gmax_frame_indexes: &[usize],
    interpolator: Interpolator,
    physical_param: PhysicalParam,
    iteration_method: IterMethod,
) -> Array2<f64> {
    let dt = 1.0 / frame_rate as f64;
    let shape = interpolator.shape();
    let shape = (shape.0 as usize, shape.1 as usize);

    let PhysicalParam {
        gmax_temperature: tw,
        solid_thermal_conductivity: k,
        solid_thermal_diffusivity: a,
        characteristic_length,
        air_thermal_conductivity,
    } = physical_param;

    let equation =
        move |point_data: PointData, h| point_data.heat_transfer_equation(h, dt, k, a, tw);

    let h1 = match iteration_method {
        IterMethod::NewtonTangent { h0, max_iter_num } => solve_core(
            gmax_frame_indexes,
            interpolator,
            newtow_tangent(equation, h0, max_iter_num),
        ),
        IterMethod::NewtonDown { h0, max_iter_num } => solve_core(
            gmax_frame_indexes,
            interpolator,
            newtow_down(equation, h0, max_iter_num),
        ),
    };
    assert_eq!(shape.0 * shape.1, h1.len());
    Array2::from_shape_vec(shape, h1).unwrap() * characteristic_length / air_thermal_conductivity
}

fn solve_core<F>(
    gmax_frame_indexes: &[usize],
    interpolator: Interpolator,
    solve_single_point: F,
) -> Vec<f64>
where
    F: Fn(PointData) -> f64 + Send + Sync,
{
    const FIRST_FEW_TO_CAL_T0: usize = 4;
    gmax_frame_indexes
        .par_iter()
        .enumerate()
        .map(|(point_index, &gmax_frame_index)| {
            if gmax_frame_index <= FIRST_FEW_TO_CAL_T0 {
                return NAN;
            }
            let temperatures = interpolator.interp_point(point_index);
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

    fn new_temps() -> Vec<f64> {
        let daq_data = daq::read_daq("./testdata/imp_20000_1.lvm").unwrap();
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
        println!("simd:\t\t\t{r1:?}\niter_no_simd:\t\t{r2:?}\nno_iter_no_simd:\t{r3:?}\n");

        assert_relative_eq!(r1.0, r2.0, max_relative = 1e-6);
        assert_relative_eq!(r1.1, r2.1, max_relative = 1e-6);
        assert_relative_eq!(r1.0, r3.0, max_relative = 1e-6);
        assert_relative_eq!(r1.1, r3.1, max_relative = 1e-6);
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
}
