use std::sync::Once;

use tracing_subscriber::fmt::format::FmtSpan;

pub fn init() {
    static START: Once = Once::new();
    START.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
            .with_target(false)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set global default tracing subscriber");
    });
}