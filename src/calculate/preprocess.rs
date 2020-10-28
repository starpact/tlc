use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

/// *traverse along the timeline to detect the peak of green values and record that frame index*
/// ### Argument:
/// green values 2D matrix
/// ### Return:
/// frame indexes of maximal green values
pub fn detect_peak(g2d: Array2<u8>) -> Array1<usize> {
    let mut peak_frames = Vec::with_capacity(g2d.ncols());

    g2d.axis_iter(Axis(1))
        .into_par_iter()
        // .map(|column| column.iter().enumerate().max_by_key(|x| x.1).unwrap().0)
        .map(|column| column.iter().enumerate().fold((0, 0, 0), |(mi_l, mi_r, mg), (i, &g)| {
            if g > mg { (i, i, g) } else if g == mg { (mi_l, i, g) } else { (mi_l, mi_r, mg) }
        }))
        .map(|x| (x.0 + x.1) >> 1)
        .collect_into_vec(&mut peak_frames);

    Array1::from(peak_frames)
}

/// *linearly interpolate the temperature along axis **X***
/// ### Argument:
/// 2D matrix of the delta temperatures between adjacent frames
///
/// realtative x positions of thermocouples in the calculation region
///
/// width of calculation region
/// ### Return:
/// 2D matrix of the interpolated temperatures
pub fn interp_x(t2d: ArrayView2<f64>, tc_x: &Vec<i32>, cal_w: usize) -> Array2<f64> {
    let mut interp_x_t2d = Array2::<f64>::zeros((t2d.nrows(), cal_w));

    par_azip!((row0 in t2d.axis_iter(Axis(0)), mut row1 in interp_x_t2d.axis_iter_mut(Axis(0))) {
        let mut iter = row1.iter_mut();
        let mut curr = 0;
        let (mut left, mut right) = (tc_x[curr], tc_x[curr + 1]);
        for i in 0..cal_w as i32 {
            if i == right && curr + 2 < tc_x.len() {
                curr += 1;
                left = tc_x[curr];
                right = tc_x[curr + 1];
            }
            *iter.next().unwrap() = (row0[curr] * (right - i) as f64
                + row0[curr + 1] * (i - left) as f64) / (right - left) as f64;
        }
    });

    interp_x_t2d
}
