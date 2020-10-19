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
        .map(|column| column.iter().enumerate().max_by_key(|x| x.1).unwrap().0)
        .collect_into_vec(&mut peak_frames);

    Array1::from(peak_frames)
}
