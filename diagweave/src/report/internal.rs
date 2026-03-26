#[cfg(feature = "trace")]
use super::types::ReportTrace;
use super::types::{Attachment, AttachmentValue, CauseCollectOptions, ReportMetadata};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::Display;

#[cfg(feature = "std")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(feature = "std")]
use std::sync::OnceLock;

/// Traversal state observed during cause collection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CauseTraversalState {
    /// Whether the traversal was truncated due to depth limit.
    pub truncated: bool,
    /// Whether a circular reference cycle was detected.
    pub cycle_detected: bool,
}

/// The current stage of source error iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceErrorIterStage {
    /// Iterating through explicitly attached source errors.
    Attached,
    /// Iterating through the natural `source()` chain of the inner error.
    Inner,
    /// Completed iteration.
    Done,
}

/// A streamed attachment item for visitor-based traversal.
#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentVisit<'a> {
    Context {
        key: &'a Cow<'static, str>,
        value: &'a AttachmentValue,
    },
    Note {
        message: &'a Cow<'static, str>,
    },
    Payload {
        name: &'a Cow<'static, str>,
        value: &'a AttachmentValue,
        media_type: Option<&'a Cow<'static, str>>,
    },
}

/// Iterator over source errors with depth/cycle control.
pub struct ReportSourceErrorIter<'a> {
    pub(crate) source_errors: core::slice::Iter<'a, Box<dyn Error + 'static>>,
    pub(crate) root_source: Option<&'a (dyn Error + 'static)>,
    pub(crate) current: Option<&'a (dyn Error + 'static)>,
    pub(crate) stage: SourceErrorIterStage,
    pub(crate) depth: usize,
    pub(crate) options: CauseCollectOptions,
    pub(crate) seen: SeenErrorAddrs,
    pub(crate) state: CauseTraversalState,
}

impl<'a> ReportSourceErrorIter<'a> {
    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.state
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = &'a (dyn Error + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        if self.state.truncated || self.state.cycle_detected {
            self.stage = SourceErrorIterStage::Done;
            return None;
        }

        loop {
            let err = match self.current.take() {
                Some(err) => err,
                None => match self.stage {
                    SourceErrorIterStage::Attached => {
                        if let Some(err) = self.source_errors.next() {
                            err.as_ref()
                        } else {
                            self.stage = SourceErrorIterStage::Inner;
                            continue;
                        }
                    }
                    SourceErrorIterStage::Inner => {
                        let Some(err) = self.root_source.take() else {
                            self.stage = SourceErrorIterStage::Done;
                            return None;
                        };
                        err
                    }
                    SourceErrorIterStage::Done => return None,
                },
            };

            if self.depth >= self.options.max_depth {
                self.state.truncated = true;
                self.stage = SourceErrorIterStage::Done;
                return None;
            }
            if self.options.detect_cycle {
                let ptr = (err as *const dyn Error) as *const ();
                let addr = ptr as usize;
                if !self.seen.insert(addr) {
                    self.state.cycle_detected = true;
                    self.stage = SourceErrorIterStage::Done;
                    return None;
                }
            }
            self.current = err.source();
            self.depth += 1;
            return Some(err);
        }
    }
}

#[derive(Default)]
pub(crate) struct ColdData {
    pub(crate) metadata: ReportMetadata,
    pub(crate) diagnostics: DiagnosticBag,
}

#[derive(Default)]
pub(crate) struct DiagnosticBag {
    #[cfg(feature = "trace")]
    pub(crate) trace: ReportTrace,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) display_causes: Vec<Box<dyn Display + 'static>>,
    pub(crate) source_errors: Vec<Box<dyn Error + 'static>>,
}

pub(crate) const EMPTY_REPORT_METADATA: ReportMetadata = ReportMetadata {
    error_code: None,
    severity: None,
    category: None,
    retryable: None,
    stack_trace: None,
    display_causes: None,
    source_errors: None,
};

/// Global context information that can be injected into reports.
#[derive(Debug, Clone, Default)]
pub struct GlobalContext {
    /// Context key-value pairs.
    pub context: Vec<(Cow<'static, str>, AttachmentValue)>,
    /// Global trace ID if available.
    #[cfg(feature = "trace")]
    pub trace_id: Option<Cow<'static, str>>,
    /// Global span ID if available.
    #[cfg(feature = "trace")]
    pub span_id: Option<Cow<'static, str>>,
    /// Global parent span ID if available.
    #[cfg(feature = "trace")]
    pub parent_span_id: Option<Cow<'static, str>>,
}

/// Context injector type alias for global context providers.
#[cfg(feature = "std")]
pub(crate) type ContextInjector = dyn Fn() -> Option<GlobalContext> + Send + Sync + 'static;

#[cfg(feature = "std")]
pub(crate) fn global_context_injector() -> &'static OnceLock<Box<ContextInjector>> {
    static INJECTOR: OnceLock<Box<ContextInjector>> = OnceLock::new();
    &INJECTOR
}

/// Error returned when global context registration fails.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterGlobalContextError;

/// Registers a global context injector that will be invoked for every new report.
#[cfg(feature = "std")]
pub fn register_global_injector(
    injector: impl Fn() -> Option<GlobalContext> + Send + Sync + 'static,
) -> Result<(), RegisterGlobalContextError> {
    global_context_injector()
        .set(Box::new(injector))
        .map_err(|_| RegisterGlobalContextError)
}

#[cfg(feature = "std")]
pub(crate) fn apply_global_context(
    attachments: &mut Vec<Attachment>,
    #[cfg(feature = "trace")] trace_ctx: &mut super::types::TraceContext,
) {
    let Some(injector) = global_context_injector().get() else {
        return;
    };
    let injected = catch_unwind(AssertUnwindSafe(injector));
    let Some(global) = injected.unwrap_or_default() else {
        return;
    };
    for (key, value) in global.context {
        attachments.push(Attachment::context(key, value));
    }
    #[cfg(feature = "trace")]
    {
        if trace_ctx.trace_id.is_none() {
            trace_ctx.trace_id = global.trace_id;
        }
        if trace_ctx.span_id.is_none() {
            trace_ctx.span_id = global.span_id;
        }
        if trace_ctx.parent_span_id.is_none() {
            trace_ctx.parent_span_id = global.parent_span_id;
        }
    }
}

pub(crate) struct SeenErrorAddrs {
    inline: [usize; 8],
    len: usize,
    spill: Vec<usize>,
}

impl SeenErrorAddrs {
    pub(crate) fn new() -> Self {
        Self {
            inline: [0usize; 8],
            len: 0,
            spill: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, addr: usize) -> bool {
        if self.contains(addr) {
            return false;
        }
        if self.len < self.inline.len() {
            self.inline[self.len] = addr;
            self.len += 1;
            return true;
        }
        self.spill.push(addr);
        true
    }

    pub(crate) fn contains(&self, addr: usize) -> bool {
        if self.inline[..self.len].contains(&addr) {
            return true;
        }
        self.spill.contains(&addr)
    }
}
