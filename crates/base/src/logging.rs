use std::sync::Once;
use tracing_error::ErrorLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let fmt_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::builder().parse_lossy("warn"));

        tracing_subscriber::registry()
            .with(ErrorLayer::default().with_filter(LevelFilter::INFO))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(fmt_filter), /*.with_span_events(FmtSpan::ENTER)*/
            )
            .init();
    });
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::error!($($arg)*);
    };
}

pub use log_error;
pub use tracing::{debug, error, info, info_span, instrument, trace, warn};
