use std::path::PathBuf;

use anyhow::Result;
use dwt::{transform, wavelet::Wavelet, Operation};
use median::Filter;
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::progress_bar::ProgressBar;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub enum FilterMethod {
    #[default]
    No,
    Median {
        window_size: usize,
    },
    Wavelet {
        threshold_ratio: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterMetadata {
    pub filter_method: FilterMethod,
    pub video_path: PathBuf,
}

#[instrument(skip(green2, progress_bar), err)]
pub fn filter_all(
    green2: ArcArray2<u8>,
    filter_method: FilterMethod,
    progress_bar: &ProgressBar,
) -> Result<ArcArray2<u8>> {
    let total = green2.dim().1;
    let _reset_guard = progress_bar.start(total as u32);

    use FilterMethod::*;
    match filter_method {
        No => {
            progress_bar.add(total as i64).unwrap();
            Ok(green2)
        }
        Median { window_size } => apply(green2, progress_bar, move |g1| median(g1, window_size)),
        Wavelet { threshold_ratio } => {
            apply(green2, progress_bar, move |g1| wavelet(g1, threshold_ratio))
        }
    }
}

#[instrument(skip(green1))]
pub fn filter_single_point(filter_method: FilterMethod, green1: ArrayView1<u8>) -> Vec<u8> {
    let mut green1 = green1.to_owned();

    use FilterMethod::*;
    match filter_method {
        No => {}
        Median { window_size } => median(green1.view_mut(), window_size),
        Wavelet { threshold_ratio } => wavelet(green1.view_mut(), threshold_ratio),
    };

    green1.to_vec()
}

fn apply<F>(mut green2: ArcArray2<u8>, progress_bar: &ProgressBar, f: F) -> Result<ArcArray2<u8>>
where
    F: Fn(ArrayViewMut1<u8>) + Send + Sync,
{
    green2
        .axis_iter_mut(Axis(1)) // green2 is cloned here
        .into_par_iter()
        .try_for_each(|green_history| {
            f(green_history);
            progress_bar.add(1)
        })?;

    Ok(green2)
}

fn median(mut green1: ArrayViewMut1<u8>, window_size: usize) {
    let mut filter = Filter::new(window_size);
    green1.iter_mut().for_each(|g| *g = filter.consume(*g));
}

/// Refer to [pywavelets](https://pywavelets.readthedocs.io/en/latest/ref).
fn wavelet(mut green1: ArrayViewMut1<u8>, threshold_ratio: f64) {
    let data_len = green1.len();
    let wavelet = db8();

    let max_level = ((data_len / (wavelet.length - 1)) as f64).log2() as usize;
    let level_2 = 1 << max_level;
    let filter_len = data_len / level_2 * level_2;

    // [u8] => [f64]
    let mut green1f: Vec<_> = green1.iter().take(filter_len).map(|v| *v as f64).collect();

    // Decomposition.
    transform(
        &mut green1f[..filter_len],
        Operation::Forward,
        &wavelet,
        max_level,
    );

    let mut start = filter_len / (1 << max_level);
    for _ in 0..max_level {
        let end = start << 1;
        let m = green1f[start..end].iter().fold(0., |m, &v| f64::max(m, v));
        let threshold = m * threshold_ratio;
        for v in &mut green1f[start..end] {
            *v = v.signum() * f64::max(v.abs() - threshold, 0.);
        }
        start = end;
    }

    // Reconstruction.
    transform(
        &mut green1f[..filter_len],
        Operation::Inverse,
        &wavelet,
        max_level,
    );

    // [f64] => [u8]
    green1
        .iter_mut()
        .zip(green1f)
        .for_each(|(g, b)| *g = b as u8);
}

/// Refer to [Daubechies 8](http://wavelets.pybytes.com/wavelet/db8)。
/// Horizontal flip.
#[inline]
fn db8() -> Wavelet<f64> {
    #[rustfmt::skip]
    let lo = vec![
        -0.00011747678400228192, 0.0006754494059985568,
        -0.0003917403729959771,  -0.00487035299301066,
        0.008746094047015655,    0.013981027917015516,
        -0.04408825393106472,    -0.01736930100202211,
        0.128747426620186,       0.00047248457399797254,
        -0.2840155429624281,     -0.015829105256023893,
        0.5853546836548691,      0.6756307362980128,
        0.3128715909144659,      0.05441584224308161,
    ];
    #[rustfmt::skip]
    let hi = vec![
        -0.05441584224308161,    0.3128715909144659,
        -0.6756307362980128,     0.5853546836548691,
        0.015829105256023893,    -0.2840155429624281,
        -0.00047248457399797254, 0.128747426620186,
        0.01736930100202211,     -0.04408825393106472,
        -0.013981027917015516,   0.008746094047015655,
        0.00487035299301066,     -0.0003917403729959771,
        -0.0006754494059985568,  -0.00011747678400228192,
    ];

    Wavelet {
        length: lo.len(),
        offset: 0,
        dec_lo: lo.clone(),
        dec_hi: hi.clone(),
        rec_lo: lo,
        rec_hi: hi,
    }
}
