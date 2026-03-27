#![allow(dead_code, unused_imports)]
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
#[cfg(feature = "std")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "std")]
use std::sync::{Mutex, MutexGuard, OnceLock};

use diagweave::prelude::*;
#[cfg(feature = "trace")]
pub use diagweave::prelude::{TraceEvent, TraceEventAttribute, TraceEventLevel};
#[cfg(feature = "tracing")]
pub use diagweave::render::DiagnosticIr;
pub use diagweave::render::PrettyIndent;
#[cfg(feature = "json")]
pub use diagweave::render::{Json, REPORT_JSON_SCHEMA_VERSION, report_json_schema};
#[cfg(feature = "std")]
pub use diagweave::report::GlobalContext;
#[cfg(feature = "std")]
pub use diagweave::report::register_global_injector;
pub use diagweave::report::{
    Attachment, AttachmentValue, ReportMetadata, StackTrace, StackTraceFormat,
};
#[cfg(feature = "tracing")]
pub use diagweave::trace::TracingExporterTrait;

/// An error type for authentication failures.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        #[allow(unconditional_recursion)]
        Some(self)
    }
}

/// A minimal renderer implementation for testing.
#[derive(Clone, Copy)]
pub struct TinyRenderer;

impl<E> ReportRenderer<E> for TinyRenderer
where
    E: Display,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "tiny: {}", report.inner())
    }
}

#[cfg(feature = "std")]
pub static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
#[cfg(feature = "std")]
pub static INJECT_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "std")]
pub static INJECTOR_INSTALLED: OnceLock<()> = OnceLock::new();

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
                .push(("request_id".into(), AttachmentValue::from("req-42")));
            #[cfg(feature = "trace")]
            {
                context.trace_id =
                    Some(TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap());
                context.span_id = Some(SpanId::new("00f067aa0ba902b7").unwrap());
            }
            Some(context)
        });
    });
}

/// Initializes the test environment, including locks and global state.
/// Returns a guard that should be held for the duration of the test to ensure isolation.
#[must_use]
#[cfg(feature = "std")]
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
