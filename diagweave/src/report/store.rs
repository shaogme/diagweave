use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::error::Error;

use super::types::{CauseCollectOptions, CauseCollection};

/// A trait for storing and collecting cause information in a report.
pub trait CauseStore: Default {
    /// The type of cause stored in this store.
    type Cause;

    /// Pushes a new cause into the store.
    fn push(&mut self, cause: Self::Cause);

    /// Extends the store with multiple causes.
    fn extend<I>(&mut self, causes: I)
    where
        I: IntoIterator<Item = Self::Cause>,
    {
        for cause in causes {
            self.push(cause);
        }
    }

    /// Returns the first error in the cause chain if available.
    fn first_error(&self) -> Option<&(dyn Error + 'static)>;

    /// Collects all causes into a [`CauseCollection`] based on the provided options.
    fn collect(&self, options: CauseCollectOptions) -> CauseCollection;
}

/// A store that supports recording event-style causes.
pub trait EventCauseStore: CauseStore {
    /// Creates an event cause from a message string.
    fn event_cause(message: String) -> Self::Cause;
}

/// A store that supports recording standard `Error` causes that are `Send + Sync + 'static`.
pub trait StdErrorCauseStore: CauseStore {
    /// Creates a cause from a standard `Error`.
    fn std_error_cause(err: Box<dyn Error + Send + Sync + 'static>) -> Self::Cause;
}

/// A store that supports recording local `Error` causes (not necessarily `Send` or `Sync`).
pub trait LocalErrorCauseStore: CauseStore {
    /// Creates a cause from a local `Error`.
    fn local_error_cause(err: Box<dyn Error + 'static>) -> Self::Cause;
}

/// Represents a standard cause, which can be an error, an event, or a group of causes.
#[derive(Debug)]
pub enum StdCause {
    /// A standard error cause.
    Error(Box<dyn Error + Send + Sync + 'static>),
    /// An event message cause.
    Event(String),
    /// A grouping of multiple causes.
    Group(Vec<StdCause>),
}

impl StdCause {
    /// Creates a new error cause.
    pub fn error(err: impl Error + Send + Sync + 'static) -> Self {
        Self::Error(Box::new(err))
    }

    /// Creates a new event cause.
    pub fn event(message: impl Into<String>) -> Self {
        Self::Event(message.into())
    }

    /// Creates a new group cause.
    pub fn group(causes: impl IntoIterator<Item = StdCause>) -> Self {
        Self::Group(causes.into_iter().collect())
    }
}

/// A node in the cause tree, currently synonymous with [`StdCause`].
pub type CauseNode = StdCause;

/// A store for standard causes.
#[derive(Debug, Default)]
pub struct StdCauseStore {
    causes: Vec<StdCause>,
}

impl StdCauseStore {
    /// Returns the list of causes in the store.
    pub fn causes(&self) -> &[StdCause] {
        &self.causes
    }
}

impl CauseStore for StdCauseStore {
    type Cause = StdCause;

    fn push(&mut self, cause: Self::Cause) {
        self.causes.push(cause);
    }

    fn first_error(&self) -> Option<&(dyn Error + 'static)> {
        first_error_std(&self.causes)
    }

    fn collect(&self, options: CauseCollectOptions) -> CauseCollection {
        let mut state = CauseCollection::default();
        let mut depth = 0usize;
        let mut seen = BTreeSet::<usize>::new();
        for cause in &self.causes {
            collect_std_cause(cause, options, &mut state, &mut depth, &mut seen);
            if state.truncated || state.cycle_detected {
                break;
            }
        }
        state
    }
}

impl EventCauseStore for StdCauseStore {
    fn event_cause(message: String) -> Self::Cause {
        StdCause::Event(message)
    }
}

impl StdErrorCauseStore for StdCauseStore {
    fn std_error_cause(err: Box<dyn Error + Send + Sync + 'static>) -> Self::Cause {
        StdCause::Error(err)
    }
}

impl LocalErrorCauseStore for StdCauseStore {
    fn local_error_cause(err: Box<dyn Error + 'static>) -> Self::Cause {
        StdCause::Event(format!("local-error: {err}"))
    }
}

/// Represents a local cause, which can be an error (including non-Send/Sync), an event, or a group.
#[derive(Debug)]
pub enum LocalCause {
    /// A local error cause.
    Error(Box<dyn Error + 'static>),
    /// An event message cause.
    Event(String),
    /// A grouping of multiple local causes.
    Group(Vec<LocalCause>),
}

impl LocalCause {
    /// Creates a new local error cause.
    pub fn error(err: impl Error + 'static) -> Self {
        Self::Error(Box::new(err))
    }

    /// Creates a new local event cause.
    pub fn event(message: impl Into<String>) -> Self {
        Self::Event(message.into())
    }

    /// Creates a new local group cause.
    pub fn group(causes: impl IntoIterator<Item = LocalCause>) -> Self {
        Self::Group(causes.into_iter().collect())
    }
}

/// A store for local causes.
#[derive(Debug, Default)]
pub struct LocalCauseStore {
    causes: Vec<LocalCause>,
}

impl LocalCauseStore {
    /// Returns the list of causes in the store.
    pub fn causes(&self) -> &[LocalCause] {
        &self.causes
    }
}

impl CauseStore for LocalCauseStore {
    type Cause = LocalCause;

    fn push(&mut self, cause: Self::Cause) {
        self.causes.push(cause);
    }

    fn first_error(&self) -> Option<&(dyn Error + 'static)> {
        first_error_local(&self.causes)
    }

    fn collect(&self, options: CauseCollectOptions) -> CauseCollection {
        let mut state = CauseCollection::default();
        let mut depth = 0usize;
        let mut seen = BTreeSet::<usize>::new();
        for cause in &self.causes {
            collect_local_cause(cause, options, &mut state, &mut depth, &mut seen);
            if state.truncated || state.cycle_detected {
                break;
            }
        }
        state
    }
}

impl EventCauseStore for LocalCauseStore {
    fn event_cause(message: String) -> Self::Cause {
        LocalCause::Event(message)
    }
}

impl StdErrorCauseStore for LocalCauseStore {
    fn std_error_cause(err: Box<dyn Error + Send + Sync + 'static>) -> Self::Cause {
        LocalCause::Error(err)
    }
}

impl LocalErrorCauseStore for LocalCauseStore {
    fn local_error_cause(err: Box<dyn Error + 'static>) -> Self::Cause {
        LocalCause::Error(err)
    }
}

/// A store that only records event messages, ignoring error objects.
#[derive(Debug, Default)]
pub struct EventOnlyStore {
    events: Vec<String>,
}

impl EventOnlyStore {
    /// Returns the list of events in the store.
    pub fn events(&self) -> &[String] {
        &self.events
    }
}

impl CauseStore for EventOnlyStore {
    type Cause = String;

    fn push(&mut self, cause: Self::Cause) {
        self.events.push(cause);
    }

    fn first_error(&self) -> Option<&(dyn Error + 'static)> {
        None
    }

    fn collect(&self, options: CauseCollectOptions) -> CauseCollection {
        let mut state = CauseCollection::default();
        for message in &self.events {
            if state.messages.len() >= options.max_depth {
                state.truncated = true;
                break;
            }
            state.messages.push(format!("event: {message}"));
        }
        state
    }
}

impl EventCauseStore for EventOnlyStore {
    fn event_cause(message: String) -> Self::Cause {
        message
    }
}

impl StdErrorCauseStore for EventOnlyStore {
    fn std_error_cause(err: Box<dyn Error + Send + Sync + 'static>) -> Self::Cause {
        err.to_string()
    }
}

impl LocalErrorCauseStore for EventOnlyStore {
    fn local_error_cause(err: Box<dyn Error + 'static>) -> Self::Cause {
        err.to_string()
    }
}

fn first_error_std(causes: &[StdCause]) -> Option<&(dyn Error + 'static)> {
    for cause in causes {
        match cause {
            StdCause::Error(err) => return Some(err.as_ref()),
            StdCause::Event(_) => {}
            StdCause::Group(children) => {
                if let Some(err) = first_error_std(children) {
                    return Some(err);
                }
            }
        }
    }
    None
}

fn first_error_local(causes: &[LocalCause]) -> Option<&(dyn Error + 'static)> {
    for cause in causes {
        match cause {
            LocalCause::Error(err) => return Some(err.as_ref()),
            LocalCause::Event(_) => {}
            LocalCause::Group(children) => {
                if let Some(err) = first_error_local(children) {
                    return Some(err);
                }
            }
        }
    }
    None
}

fn collect_std_cause(
    cause: &StdCause,
    options: CauseCollectOptions,
    state: &mut CauseCollection,
    depth: &mut usize,
    seen: &mut BTreeSet<usize>,
) {
    if *depth >= options.max_depth {
        state.truncated = true;
        return;
    }
    match cause {
        StdCause::Error(err) => {
            state.messages.push(err.to_string());
            *depth += 1;
            collect_error_chain(err.source(), options, state, depth, seen);
        }
        StdCause::Event(message) => {
            state.messages.push(format!("event: {message}"));
            *depth += 1;
        }
        StdCause::Group(children) => {
            for child in children {
                collect_std_cause(child, options, state, depth, seen);
                if state.truncated || state.cycle_detected {
                    break;
                }
            }
        }
    }
}

fn collect_local_cause(
    cause: &LocalCause,
    options: CauseCollectOptions,
    state: &mut CauseCollection,
    depth: &mut usize,
    seen: &mut BTreeSet<usize>,
) {
    if *depth >= options.max_depth {
        state.truncated = true;
        return;
    }
    match cause {
        LocalCause::Error(err) => {
            state.messages.push(err.to_string());
            *depth += 1;
            collect_error_chain(err.source(), options, state, depth, seen);
        }
        LocalCause::Event(message) => {
            state.messages.push(format!("event: {message}"));
            *depth += 1;
        }
        LocalCause::Group(children) => {
            for child in children {
                collect_local_cause(child, options, state, depth, seen);
                if state.truncated || state.cycle_detected {
                    break;
                }
            }
        }
    }
}

pub(crate) fn collect_error_chain(
    start: Option<&(dyn Error + 'static)>,
    options: CauseCollectOptions,
    state: &mut CauseCollection,
    depth: &mut usize,
    seen: &mut BTreeSet<usize>,
) {
    let mut current = start;
    while let Some(err) = current {
        if *depth >= options.max_depth {
            state.truncated = true;
            break;
        }
        if options.detect_cycle {
            let ptr = (err as *const dyn Error) as *const ();
            let addr = ptr as usize;
            if !seen.insert(addr) {
                state.cycle_detected = true;
                break;
            }
        }
        state.messages.push(err.to_string());
        *depth += 1;
        current = err.source();
    }
}

/// The default cause store type, using [`StdCauseStore`].
pub type DefaultCauseStore = StdCauseStore;
