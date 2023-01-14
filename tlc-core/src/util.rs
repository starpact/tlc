pub mod log;
pub mod progress_bar;

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
