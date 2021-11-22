use std::sync::Arc;

use anyhow::Result;
use dwt::wavelet::Wavelet;
use dwt::{transform, Operation};
use median::Filter;
use ndarray::parallel::prelude::*;
use ndarray::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::util::{blocking, timing};

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub enum FilterMethod {
    No,
    Median(usize),
    Wavelet(f64),
}

impl Default for FilterMethod {
    fn default() -> Self {
        FilterMethod::No
    }
}

pub async fn filter(g2: Arc<Array2<u8>>, filter_method: FilterMethod) -> Result<Arc<Array2<u8>>> {
    let _timing = timing::start("filtering g2");
    debug!("filter method: {:?}", filter_method);

    let filtered_g2 = match filter_method {
        FilterMethod::No => return Ok(g2),
        FilterMethod::Median(window_size) => cal(g2, median_fn(window_size)).await?,
        FilterMethod::Wavelet(threshold_ratio) => cal(g2, wavelet_fn(threshold_ratio)).await?,
    };

    Ok(Arc::new(filtered_g2))
}

async fn cal<F>(g2: Arc<Array2<u8>>, f: F) -> Result<Array2<u8>>
where
    F: Fn(ArrayViewMut1<u8>) + Send + Sync + 'static,
{
    blocking::compute(move || {
        let mut filtered_g2 = g2.as_ref().clone();
        filtered_g2
            .axis_iter_mut(Axis(1))
            .into_par_iter()
            .for_each(f);

        filtered_g2
    })
    .await
}

#[inline]
fn median(mut g1: ArrayViewMut1<u8>, window_size: usize) {
    let mut filter = Filter::new(window_size);
    g1.iter_mut().for_each(|g| *g = filter.consume(*g));
}

fn median_fn(window_size: usize) -> impl Fn(ArrayViewMut1<u8>) {
    move |g1| median(g1, window_size)
}

/// Refer to [pywavelets](https://pywavelets.readthedocs.io/en/latest/ref)
#[inline]
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

fn wavelet_fn(threshold_ratio: f64) -> impl Fn(ArrayViewMut1<u8>) {
    move |g1| wavelet(g1, threshold_ratio)
}

/// Refer to [Daubechies 8](http://wavelets.pybytes.com/wavelet/db8)。
/// Horizontal flip.
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
