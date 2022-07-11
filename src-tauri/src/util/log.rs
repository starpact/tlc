use std::sync::Once;

static START: Once = Once::new();
pub fn init() {
    START.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .pretty()
            .init();
    });
}
