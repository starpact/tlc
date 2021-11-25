use anyhow::Result;
use tokio::sync::oneshot;

pub async fn compute<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    rayon::spawn(move || {
        let _ = tx.send(f());
    });

    Ok(rx.await?)
}
