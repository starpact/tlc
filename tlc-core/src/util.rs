macro_rules! impl_eq_always_false {
    ($($T:ty),+$(,)?) => {
        $(
            impl PartialEq for $T {
                fn eq(&self, _: &Self) -> bool {
                    false
                }
            }

            impl Eq for $T {}
        )+
    };
}

pub(crate) use impl_eq_always_false;

pub(crate) mod log {
    use std::sync::Once;
    use tracing_subscriber::fmt::{format::FmtSpan, time::LocalTime};

    pub fn init() {
        static START: Once = Once::new();
        START.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .with_timer(LocalTime::rfc_3339())
                .with_max_level(tracing::Level::TRACE)
                .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
                .with_target(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("failed to set global default tracing subscriber");
        });
    }
}
