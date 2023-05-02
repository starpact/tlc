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
    use tracing::Level;
    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

    pub fn init() {
        let builder = tracing_subscriber::fmt()
            .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(Level::TRACE.into())
                    .from_env()
                    .unwrap(),
            );

        // This has to be executed in single threaded environment.
        #[cfg(not(test))]
        let builder = builder
            .with_timer(tracing_subscriber::fmt::time::OffsetTime::local_rfc_3339().unwrap());

        let subscriber = builder.finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set global default tracing subscriber");
    }
}
