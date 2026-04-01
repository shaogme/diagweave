use std::error::Error;
use std::fmt::{Display, Formatter};
#[cfg(feature = "std")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "std")]
use std::sync::{Mutex, MutexGuard, OnceLock};

use diagweave::prelude::*;
#[cfg(feature = "std")]
use diagweave::report::GlobalContext;
#[cfg(all(feature = "std", feature = "trace"))]
use diagweave::report::GlobalTraceContext;
#[cfg(feature = "std")]
use diagweave::report::register_global_injector;

/// An error type for authentication failures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthError {
    InvalidToken,
}

impl Display for AuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "auth invalid token"),
        }
    }
}

impl Error for AuthError {}

/// An error type for API failures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ApiError {
    Unauthorized,
    Wrapped { code: u16 },
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "api unauthorized"),
            Self::Wrapped { code } => write!(f, "api wrapped code={code}"),
        }
    }
}

impl Error for ApiError {}

/// An error type used to test recursive source detection.
#[derive(Debug)]
pub struct LoopError;

impl Display for LoopError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "loop error")
    }
}

impl Error for LoopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self)
    }
}

/// A minimal renderer implementation for testing.
#[derive(Clone, Copy)]
pub struct TinyRenderer;

impl<E, State> ReportRenderer<E, State> for TinyRenderer
where
    E: Display,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "tiny: {}", report.inner())
    }
}

const _: fn() = || {
    let _ = LoopError;
    let _ = TinyRenderer;
    let _ = ApiError::Wrapped { code: 0 };
};

#[cfg(feature = "std")]
pub static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
#[cfg(feature = "std")]
pub static INJECT_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "std")]
pub static INJECTOR_INSTALLED: OnceLock<()> = OnceLock::new();

#[cfg(feature = "std")]
const _: fn() = || {
    let _ = INJECT_ENABLED.load(Ordering::Relaxed);
    let _ = INJECTOR_INSTALLED.get();
    let _f: fn() = ensure_global_injector_installed;
    let _ = _f;
};

/// Ensures that the global injector is installed for tests.
#[cfg(feature = "std")]
pub fn ensure_global_injector_installed() {
    let _ = INJECTOR_INSTALLED.get_or_init(|| {
        let _ = register_global_injector(|| {
            if !INJECT_ENABLED.load(Ordering::Relaxed) {
                return None;
            }
            let mut context = GlobalContext::default();
            context
                .context
                .insert("request_id", ContextValue::from("req-42"));
            #[cfg(feature = "trace")]
            {
                let trace_id = TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").ok();
                let span_id = SpanId::new("00f067aa0ba902b7").ok();
                context.trace = Some(GlobalTraceContext {
                    trace_id,
                    span_id,
                    ..GlobalTraceContext::default()
                });
            }
            Some(context)
        });
    });
}

/// Initializes the test environment, including locks and global state.
/// Returns a guard that should be held for the duration of the test to ensure isolation.
#[must_use]
#[cfg(feature = "std")]
#[allow(dead_code)]
pub fn init_test() -> Option<MutexGuard<'static, ()>> {
    Some(
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock"),
    )
}

/// Initializes the test environment. (no-std version)
#[must_use]
#[cfg(not(feature = "std"))]
pub fn init_test() -> Option<()> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ensure_init_test_used() {
        let _ = init_test();
    }
}
