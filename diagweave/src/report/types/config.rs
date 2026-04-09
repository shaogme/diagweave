use core::fmt;

#[cfg(feature = "std")]
use std::sync::OnceLock;

/// Profile-dependent default values for configuration options.
///
/// This struct provides a centralized location for all profile-dependent defaults,
/// ensuring consistency across the codebase and making it easy to audit default values.
///
/// # Profile-Dependent Behavior
///
/// Default values change based on build profile to provide better debugging
/// experience during development while optimizing for performance in production:
///
/// | Option | Debug Build | Release Build |
/// |--------|-------------|---------------|
/// | `accumulate_source_chain` | `true` | `false` |
/// | `detect_cycle` | `true` | `false` |
/// | `max_depth` | `16` | `16` |
pub struct ProfileDefaults;

impl ProfileDefaults {
    /// Returns the default value for `accumulate_source_chain` based on build profile.
    ///
    /// In debug builds, returns `true` to enable source chain accumulation for better debugging.
    /// In release builds, returns `false` for better performance.
    #[inline]
    pub const fn accumulate_source_chain() -> bool {
        cfg!(debug_assertions)
    }

    /// Returns the default value for `detect_cycle` based on build profile.
    ///
    /// In debug builds, returns `true` to enable cycle detection for safety.
    /// In release builds, returns `false` for performance.
    #[inline]
    pub const fn detect_cycle() -> bool {
        cfg!(debug_assertions)
    }

    /// Returns the default value for `max_depth`.
    ///
    /// This value is consistent across all build profiles.
    #[inline]
    pub const fn max_depth() -> usize {
        16
    }
}

/// Configuration resolution context for determining effective values.
///
/// This enum represents the source of a resolved configuration value,
/// following the priority chain: Report > Global > Profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Value was set at the report level (highest priority).
    Report,
    /// Value was set in global configuration.
    Global,
    /// Value is the profile-dependent default (lowest priority).
    Profile,
}

/// Resolved configuration value with its source.
///
/// This tuple struct pairs a resolved value with its source, useful for
/// debugging and auditing configuration resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedValue<T> {
    /// The resolved configuration value.
    pub value: T,
    /// The source from which the value was resolved.
    pub source: ConfigSource,
}

impl<T> ResolvedValue<T> {
    /// Creates a new resolved value with the given value and source.
    #[inline]
    pub const fn new(value: T, source: ConfigSource) -> Self {
        Self { value, source }
    }

    /// Returns the resolved value.
    #[inline]
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: fmt::Display> fmt::Display for ResolvedValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (from {:?})", self.value, self.source)
    }
}

/// Global configuration for Report behavior.
///
/// This provides application-wide defaults for [`ReportOptions`] fields.
/// Values set here will be used when a [`Report`] doesn't have its own
/// [`ReportOptions`] set for a particular field.
///
/// # Configuration Priority
///
/// 1. Report-level `ReportOptions` (highest priority)
/// 2. `GlobalConfig` (this struct)
/// 3. Profile-dependent defaults (lowest priority)
///
/// # Example
///
/// ```rust
/// use diagweave::report::{GlobalConfig, set_global_config};
///
/// // Set global defaults for your application
/// let config = GlobalConfig::new()
///     .with_accumulate_source_chain(true)
///     .with_max_depth(32)
///     .with_cycle_detection(true);
///
/// set_global_config(config);
/// ```
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalConfig {
    /// Default value for `accumulate_source_chain` when not set in ReportOptions.
    pub accumulate_source_chain: bool,
    /// Default value for `max_depth` when not set in ReportOptions.
    pub max_depth: usize,
    /// Default value for `detect_cycle` when not set in ReportOptions.
    pub detect_cycle: bool,
}

#[cfg(feature = "std")]
impl GlobalConfig {
    /// Creates a new GlobalConfig with profile-dependent defaults.
    ///
    /// # Profile-Dependent Defaults
    ///
    /// | Option | Debug Build | Release Build |
    /// |--------|-------------|---------------|
    /// | `accumulate_source_chain` | `true` | `false` |
    /// | `detect_cycle` | `true` | `false` |
    /// | `max_depth` | `16` | `16` |
    #[inline]
    pub const fn new() -> Self {
        Self {
            accumulate_source_chain: ProfileDefaults::accumulate_source_chain(),
            max_depth: ProfileDefaults::max_depth(),
            detect_cycle: ProfileDefaults::detect_cycle(),
        }
    }

    /// Sets the default for `accumulate_source_chain`.
    #[inline]
    pub const fn with_accumulate_source_chain(mut self, accumulate: bool) -> Self {
        self.accumulate_source_chain = accumulate;
        self
    }

    /// Sets the default for `max_depth`.
    #[inline]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Sets the default for `detect_cycle`.
    #[inline]
    pub const fn with_cycle_detection(mut self, detect_cycle: bool) -> Self {
        self.detect_cycle = detect_cycle;
        self
    }

    /// Resolves the `accumulate_source_chain` value with source tracking.
    #[inline]
    pub fn resolve_accumulate_source_chain_with_source(&self) -> ResolvedValue<bool> {
        ResolvedValue::new(self.accumulate_source_chain, ConfigSource::Global)
    }

    /// Resolves the `max_depth` value with source tracking.
    #[inline]
    pub fn resolve_max_depth_with_source(&self) -> ResolvedValue<usize> {
        ResolvedValue::new(self.max_depth, ConfigSource::Global)
    }

    /// Resolves the `detect_cycle` value with source tracking.
    #[inline]
    pub fn resolve_detect_cycle_with_source(&self) -> ResolvedValue<bool> {
        ResolvedValue::new(self.detect_cycle, ConfigSource::Global)
    }

    /// Returns the `accumulate_source_chain` value.
    #[inline]
    pub fn resolve_accumulate_source_chain(&self) -> bool {
        self.accumulate_source_chain
    }

    /// Returns the `max_depth` value.
    #[inline]
    pub fn resolve_max_depth(&self) -> usize {
        self.max_depth
    }

    /// Returns the `detect_cycle` value.
    #[inline]
    pub fn resolve_detect_cycle(&self) -> bool {
        self.detect_cycle
    }

    /// Returns the global configuration.
    ///
    /// If no configuration has been set, returns a default config with profile-dependent defaults.
    #[inline]
    pub fn global() -> &'static Self {
        GLOBAL_CONFIG.get_or_init(|| Self::new())
    }

    fn set_global(self) -> Result<(), SetGlobalConfigError> {
        GLOBAL_CONFIG.set(self).map_err(|_| SetGlobalConfigError)
    }
}

#[cfg(feature = "std")]
impl Default for GlobalConfig {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when global config registration fails.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetGlobalConfigError;

#[cfg(feature = "std")]
impl fmt::Display for SetGlobalConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("global config already set")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SetGlobalConfigError {}

/// Sets the global configuration for Report behavior.
///
/// This should be called once at application startup.
/// Returns an error if called multiple times.
///
/// # Example
///
/// ```rust
/// use diagweave::report::{GlobalConfig, set_global_config};
///
/// let config = GlobalConfig::new()
///     .with_accumulate_source_chain(true)
///     .with_max_depth(32);
///
/// set_global_config(config).expect("Global config already set");
/// ```
#[cfg(feature = "std")]
pub fn set_global_config(config: GlobalConfig) -> Result<(), SetGlobalConfigError> {
    GlobalConfig::set_global(config)
}

#[cfg(feature = "std")]
static GLOBAL_CONFIG: OnceLock<GlobalConfig> = OnceLock::new();

/// Per-report configuration for error source chain accumulation and cause collection behavior.
///
/// This controls whether [`Report::map_err()`] automatically accumulates the source error chain
/// when transforming error types, as well as options for cause collection depth and cycle detection.
///
/// # Configuration Priority
///
/// All fields are optional (`Option<T>`). The effective value is determined by:
/// 1. Report-level `ReportOptions` (if set)
/// 2. Global `GlobalConfig` (if set)
/// 3. Profile-dependent defaults
///
/// # Default Behavior (when not explicitly set)
///
/// Default values depend on the build profile to provide better debugging experience
/// during development while avoiding performance overhead in production:
///
/// | Option | Debug Build | Release Build |
/// |--------|-------------|---------------|
/// | `accumulate_source_chain` | `true` | `false` |
/// | `detect_cycle` | `true` | `false` |
/// | `max_depth` | `16` | `16` |
///
/// # Example
///
/// ```rust
/// use diagweave::prelude::Report;
/// use diagweave::report::ReportOptions;
/// use diagweave::Error;
///
/// // Create a report with default options (profile-dependent)
/// #[derive(Debug, Error)]
/// #[display("my error")]
/// struct MyError;
///
/// let my_error = MyError;
/// let report = Report::new(my_error);
///
/// // Explicitly enable source chain accumulation
/// let _report = report.set_accumulate_source_chain(true);
///
/// // Configure cause collection depth
/// let _report = _report.set_options(ReportOptions::new().with_max_depth(32));
///
/// // Disable cycle detection for performance-critical paths
/// let _report = _report.set_options(ReportOptions::new().with_cycle_detection(false));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportOptions {
    /// Whether `map_err()` should automatically accumulate source error chains.
    ///
    /// When `Some(true)`, calling `map_err()` will preserve and extend the origin
    /// source error chain.
    ///
    /// When `Some(false)`, `map_err()` only transforms the error type without
    /// accumulating source chains.
    ///
    /// When `None`, the value is inherited from [`GlobalConfig`] or profile defaults.
    ///
    /// **Default**: `None` (inherits from global config or profile default).
    pub accumulate_source_chain: Option<bool>,

    /// Maximum depth of causes to collect during source error traversal.
    ///
    /// This limits how deep the error chain will be traversed when collecting
    /// source errors. A higher value provides more complete error context but
    /// may impact performance for very deep error chains.
    ///
    /// When `None`, the value is inherited from [`GlobalConfig`] or profile defaults.
    ///
    /// **Default**: `None` (inherits from global config or `16`).
    pub max_depth: Option<usize>,

    /// Whether to detect cycles in the cause chain during traversal.
    ///
    /// When `Some(true)`, the error chain traversal will track visited errors and
    /// mark cycles when detected. This is useful for debugging but adds
    /// overhead from maintaining a visited set.
    ///
    /// When `Some(false)`, cycle detection is skipped for better performance.
    /// Use this in release builds when error chains are trusted to be acyclic.
    ///
    /// When `None`, the value is inherited from [`GlobalConfig`] or profile defaults.
    ///
    /// **Default**: `None` (inherits from global config or profile default).
    pub detect_cycle: Option<bool>,
}

impl ReportOptions {
    /// Creates new report options with all fields unset (None).
    ///
    /// All values will be inherited from [`GlobalConfig`] or profile defaults.
    #[inline]
    pub const fn new() -> Self {
        Self {
            accumulate_source_chain: None,
            max_depth: None,
            detect_cycle: None,
        }
    }

    /// Sets whether source chains should be accumulated during `map_err()`.
    #[inline]
    pub const fn with_accumulate_source_chain(mut self, accumulate: bool) -> Self {
        self.accumulate_source_chain = Some(accumulate);
        self
    }

    /// Sets the maximum depth for cause collection.
    #[inline]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    /// Enables or disables cycle detection during cause collection.
    #[inline]
    pub const fn with_cycle_detection(mut self, detect_cycle: bool) -> Self {
        self.detect_cycle = Some(detect_cycle);
        self
    }

    /// Resolves the effective value for `accumulate_source_chain` with source tracking.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_accumulate_source_chain_with_source(&self) -> ResolvedValue<bool> {
        if let Some(value) = self.accumulate_source_chain {
            return ResolvedValue::new(value, ConfigSource::Report);
        }
        #[cfg(feature = "std")]
        {
            GlobalConfig::global().resolve_accumulate_source_chain_with_source()
        }
        #[cfg(not(feature = "std"))]
        {
            ResolvedValue::new(
                ProfileDefaults::accumulate_source_chain(),
                ConfigSource::Profile,
            )
        }
    }

    /// Resolves the effective value for `max_depth` with source tracking.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_max_depth_with_source(&self) -> ResolvedValue<usize> {
        if let Some(value) = self.max_depth {
            return ResolvedValue::new(value, ConfigSource::Report);
        }
        #[cfg(feature = "std")]
        {
            GlobalConfig::global().resolve_max_depth_with_source()
        }
        #[cfg(not(feature = "std"))]
        {
            ResolvedValue::new(ProfileDefaults::max_depth(), ConfigSource::Profile)
        }
    }

    /// Resolves the effective value for `detect_cycle` with source tracking.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_detect_cycle_with_source(&self) -> ResolvedValue<bool> {
        if let Some(value) = self.detect_cycle {
            return ResolvedValue::new(value, ConfigSource::Report);
        }
        #[cfg(feature = "std")]
        {
            GlobalConfig::global().resolve_detect_cycle_with_source()
        }
        #[cfg(not(feature = "std"))]
        {
            ResolvedValue::new(ProfileDefaults::detect_cycle(), ConfigSource::Profile)
        }
    }

    /// Resolves the effective value for `accumulate_source_chain`.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_accumulate_source_chain(&self) -> bool {
        self.resolve_accumulate_source_chain_with_source().value
    }

    /// Resolves the effective value for `max_depth`.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_max_depth(&self) -> usize {
        self.resolve_max_depth_with_source().value
    }

    /// Resolves the effective value for `detect_cycle`.
    ///
    /// Priority: ReportOptions > GlobalConfig > Profile default
    #[inline]
    pub fn resolve_detect_cycle(&self) -> bool {
        self.resolve_detect_cycle_with_source().value
    }

    /// Returns a CauseCollectOptions view with resolved values for internal use.
    #[inline]
    pub(crate) fn as_cause_options(&self) -> CauseCollectOptions {
        CauseCollectOptions {
            max_depth: self.resolve_max_depth(),
            detect_cycle: self.resolve_detect_cycle(),
        }
    }

    /// Returns a static reference to the default ReportOptions.
    /// This is useful for cases where no cold data exists.
    pub(crate) const fn default_ref() -> &'static Self {
        static DEFAULT: ReportOptions = ReportOptions {
            accumulate_source_chain: None,
            max_depth: None,
            detect_cycle: None,
        };
        &DEFAULT
    }
}

impl Default for ReportOptions {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Options for collecting cause messages from an error report.
///
/// This is a lightweight view into ReportOptions for internal use.
/// It is used by the traversal iterators and internal chain-building functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CauseCollectOptions {
    /// Maximum depth of causes to collect.
    pub max_depth: usize,
    /// Whether to detect cycles in the cause chain.
    pub detect_cycle: bool,
}

impl Default for CauseCollectOptions {
    #[inline]
    fn default() -> Self {
        Self {
            max_depth: ProfileDefaults::max_depth(),
            detect_cycle: ProfileDefaults::detect_cycle(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_defaults() {
        #[cfg(debug_assertions)]
        {
            assert!(ProfileDefaults::accumulate_source_chain());
            assert!(ProfileDefaults::detect_cycle());
        }
        #[cfg(not(debug_assertions))]
        {
            assert!(!ProfileDefaults::accumulate_source_chain());
            assert!(!ProfileDefaults::detect_cycle());
        }
        assert_eq!(ProfileDefaults::max_depth(), 16);
    }

    #[test]
    fn test_report_options_resolution() {
        let opts = ReportOptions::new();
        let resolved = opts.resolve_accumulate_source_chain_with_source();
        #[cfg(feature = "std")]
        assert_eq!(resolved.source, ConfigSource::Global);
        #[cfg(not(feature = "std"))]
        assert_eq!(resolved.source, ConfigSource::Profile);

        let opts_with_value = ReportOptions::new().with_accumulate_source_chain(true);
        let resolved = opts_with_value.resolve_accumulate_source_chain_with_source();
        assert_eq!(resolved.source, ConfigSource::Report);
        assert!(resolved.value);
    }

    #[test]
    fn test_cause_collect_options_default() {
        let opts = CauseCollectOptions::default();
        assert_eq!(opts.max_depth, 16);
    }
}
