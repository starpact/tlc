extern crate test;
use approx::assert_relative_eq;
use test::Bencher;

use crate::daq;

use super::*;

impl PointData<'_> {
    fn iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
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

    fn no_iter_no_simd(&self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
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
}

fn new_temps() -> Vec<f64> {
    let daq_data = daq::read::read_daq("./testdata/imp_20000_1.lvm").unwrap();
    daq_data.column(3).to_vec()
}

const I: (f64, f64, f64, f64, f64) = (100.0, 0.04, 0.19, 1.091e-7, 35.48);

#[test]
fn test_single_point_correct() {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    let r1 = point_data.heat_transfer_equation(I.0, I.1, I.2, I.3, I.4);
    let r2 = point_data.iter_no_simd(I.0, I.1, I.2, I.3, I.4);
    let r3 = point_data.no_iter_no_simd(I.0, I.1, I.2, I.3, I.4);
    println!("simd:\t\t\t{r1:?}\niter_no_simd:\t\t{r2:?}\nno_iter_no_simd:\t{r3:?}");

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
        temperatures: &temps,
    };
    // 8,658 ns/iter (+/- 90)
    b.iter(|| point_data.heat_transfer_equation(I.0, I.1, I.2, I.3, I.4));
}

#[bench]
fn bench_iter_no_simd(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 13,194 ns/iter (+/- 194)
    b.iter(|| point_data.iter_no_simd(I.0, I.1, I.2, I.3, I.4));
}

#[bench]
fn bench_no_iter_no_simd(b: &mut Bencher) {
    let temps = new_temps();
    let point_data = PointData {
        gmax_frame_index: 800,
        temperatures: &temps,
    };
    // 13,198 ns/iter (+/- 175)
    b.iter(|| point_data.no_iter_no_simd(I.0, I.1, I.2, I.3, I.4));
}

impl Default for PhysicalParam {
    fn default() -> PhysicalParam {
        PhysicalParam {
            gmax_temperature: 35.48,
            solid_thermal_conductivity: 0.19,
            solid_thermal_diffusivity: 1.091e-7,
            characteristic_length: 0.015,
            air_thermal_conductivity: 0.0276,
        }
    }
}
