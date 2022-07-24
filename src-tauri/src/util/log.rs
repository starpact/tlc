use std::sync::Once;

use tracing_subscriber::fmt::format::FmtSpan;

static START: Once = Once::new();
pub fn init() {
    START.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_span_events(FmtSpan::CLOSE)
            .pretty()
            .init();
    });
}
