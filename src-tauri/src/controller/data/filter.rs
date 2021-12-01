use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::{anyhow, bail};
use dwt::{transform, wavelet::Wavelet, Operation};
use median::Filter;
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::util::timing;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub enum FilterMethod {
    No,
    Median(usize),
    Wavelet(f64),
}

impl Default for FilterMethod {
    fn default() -> Self {
        Self::No
    }
}

pub fn filter(
    filter_method: FilterMethod,
    g2: ArcArray2<u8>,
    progress: &AtomicI64,
) -> anyhow::Result<ArcArray2<u8>> {
    let _timing = timing::start("filtering g2");
    debug!("filter method: {:?}", filter_method);

    use FilterMethod::*;
    match filter_method {
        No => {
            // This is really stupid but I want to keep things consistent.
            progress.fetch_add(g2.dim().1 as i64, Ordering::SeqCst);
            Ok(g2)
        }
        Median(window_size) => cal(g2, move |g1| {
            median(g1, window_size);
            if progress.fetch_add(1, Ordering::SeqCst) < 0 {
                Err(())
            } else {
                Ok(())
            }
        }),
        Wavelet(threshold_ratio) => cal(g2, move |g1| {
            wavelet(g1, threshold_ratio);
            if progress.fetch_add(1, Ordering::SeqCst) < 0 {
                Err(())
            } else {
                Ok(())
            }
        }),
    }
}

pub fn filter_single_point(
    filter_method: FilterMethod,
    g2: ArcArray2<u8>,
    pos: usize,
) -> anyhow::Result<Vec<u8>> {
    if pos >= g2.dim().1 {
        bail!("index({}) out of bounds({})", pos, g2.dim().1);
    }

    let mut g1 = g2.column(pos).to_owned();
    match filter_method {
        FilterMethod::No => {}
        FilterMethod::Median(window_size) => median(g1.view_mut(), window_size),
        FilterMethod::Wavelet(threshold_ratio) => wavelet(g1.view_mut(), threshold_ratio),
    };

    Ok(g1.to_vec())
}

fn cal<F>(mut g2: ArcArray2<u8>, f: F) -> anyhow::Result<ArcArray2<u8>>
where
    F: Fn(ArrayViewMut1<u8>) -> Result<(), ()> + Send + Sync,
{
    g2.view_mut()
        .axis_iter_mut(Axis(1))
        .into_par_iter()
        .try_for_each(f)
        .map_err(|_| anyhow!("aborted"))?;

    Ok(g2)
}

fn median(mut g1: ArrayViewMut1<u8>, window_size: usize) {
    let mut filter = Filter::new(window_size);
    g1.iter_mut().for_each(|g| *g = filter.consume(*g));
}

/// Refer to [pywavelets](https://pywavelets.readthedocs.io/en/latest/ref)
fn wavelet(mut g1: ArrayViewMut1<u8>, threshold_ratio: f64) {
    let data_len = g1.len();
    let wavelet = db8();

    let max_level = ((data_len / (wavelet.length - 1)) as f64).log2() as usize;
    let level_2 = 1 << max_level;
    let filter_len = data_len / level_2 * level_2;

    // [u8] => [f64]
    let mut g1f: Vec<_> = g1.iter().take(filter_len).map(|v| *v as f64).collect();

    // Decomposition
    transform(
        &mut g1f[..filter_len],
        Operation::Forward,
        &wavelet,
        max_level,
    );

    let mut start = filter_len / (1 << max_level);
    for _ in 0..max_level {
        let end = start << 1;
        let m = g1f[start..end].iter().fold(0., |m, &v| f64::max(m, v));
        let threshold = m * threshold_ratio;
        for v in &mut g1f[start..end] {
            *v = v.signum() * f64::max(v.abs() - threshold, 0.);
        }
        start = end;
    }

    // Reconstruction
    transform(
        &mut g1f[..filter_len],
        Operation::Inverse,
        &wavelet,
        max_level,
    );

    // [f64] => [u8]
    g1.iter_mut().zip(g1f).for_each(|(g, b)| *g = b as u8);
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
