use std::cell::RefCell;

use median::Filter;

use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

use serde::{Deserialize, Serialize};

use packed_simd::f32x8;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum FilterMethod {
    No,
    Median(usize),
}

/// filter the green history of each pixel along time axis
pub fn filtering(mut g2d: ArrayViewMut2<u8>, filter_method: FilterMethod) {
    match filter_method {
        FilterMethod::Median(window_size) => {
            g2d.axis_iter_mut(Axis(1))
                .into_par_iter()
                .for_each(|mut col| {
                    let mut filter = Filter::new(window_size);
                    col.iter_mut().for_each(|g| *g = filter.consume(*g))
                });
        }
        _ => {}
    }
}

/// traverse along the timeline to detect the peak of green values and record that frame index
pub fn detect_peak(g2d: ArrayView2<u8>) -> Vec<usize> {
    let mut peak_frames = Vec::with_capacity(g2d.ncols());

    g2d.axis_iter(Axis(1))
        .into_par_iter()
        .map(|column| {
            let (first_max, last_max, _) =
                column
                    .iter()
                    .enumerate()
                    .fold((0, 0, 0), |(first_m, last_m, max_g), (i, &g)| {
                        if g > max_g {
                            (i, i, g)
                        } else if g == max_g {
                            (first_m, i, g)
                        } else {
                            (first_m, last_m, max_g)
                        }
                    });
            (first_max + last_max) >> 1
        })
        .collect_into_vec(&mut peak_frames);

    peak_frames
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum InterpMethod {
    Horizontal,
    Vertical,
    Bilinear,
}

/// interpolation of temperature matrix
pub fn interp<'a>(
    t2d: ArrayView2<'a, f32>,
    interp_method: InterpMethod,
    tc_pos: &'a [(i32, i32)],
    tl_pos: (usize, usize),
    region_shape: (usize, usize),
) -> Box<dyn Fn(&RefCell<Vec<f32>>, usize, usize) + Send + Sync + 'a> {
    match interp_method {
        InterpMethod::Horizontal | InterpMethod::Vertical => {
            interp1d(t2d, interp_method, region_shape.1, tc_pos, tl_pos)
        }
        InterpMethod::Bilinear => unimplemented!(),
    }
}

fn interp1d<'a>(
    t2d: ArrayView2<'a, f32>,
    interp_method: InterpMethod,
    cal_w: usize,
    tc_pos: &'a [(i32, i32)],
    tl_pos: (usize, usize),
) -> Box<dyn Fn(&RefCell<Vec<f32>>, usize, usize) + Send + Sync + 'a> {
    Box::new(move |temps, pos, peak_frame| {
        let (tc_pos, pos): (Vec<_>, _) = match interp_method {
            InterpMethod::Horizontal => (
                tc_pos.iter().map(|(_, x)| x - tl_pos.1 as i32).collect(),
                (pos % cal_w) as i32,
            ),
            InterpMethod::Vertical => (
                tc_pos.iter().map(|(y, _)| y - tl_pos.0 as i32).collect(),
                (pos / cal_w) as i32,
            ),
            _ => unreachable!(),
        };

        let mut l_index = 0;
        while pos > tc_pos[l_index + 1] && l_index < tc_pos.len() - 2 {
            l_index += 1;
        }
        let r_index = l_index + 1;
        let (l, r) = (tc_pos[l_index], tc_pos[r_index]);
        let (l_temps, r_temps) = (t2d.row(l_index), t2d.row(r_index));
        let l_temps = l_temps.as_slice_memory_order().unwrap();
        let r_temps = r_temps.as_slice_memory_order().unwrap();

        let mut temps = temps.borrow_mut();

        let mut frame = 0;
        while frame + f32x8::lanes() < peak_frame {
            let l_val = unsafe { f32x8::from_slice_unaligned_unchecked(&l_temps[frame..]) };
            let r_val = unsafe { f32x8::from_slice_unaligned_unchecked(&r_temps[frame..]) };
            let vec8 = (l_val * (r - pos) as f32 + r_val * (pos - l) as f32) / (r - l) as f32;
            unsafe { vec8.write_to_slice_unaligned_unchecked(&mut temps[frame..]) };
            frame += f32x8::lanes();
        }
        while frame < peak_frame {
            let (l_val, r_val) = (l_temps[frame], r_temps[frame]);
            unsafe {
                *temps.get_unchecked_mut(frame) =
                    (l_val * (r - pos) as f32 + r_val * (pos - l) as f32) / (r - l) as f32;
            }
            frame += 1;
        }
    })
}
