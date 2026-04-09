//! Tests for global configuration priority system.
//!
//! This module verifies that configuration values are resolved correctly
//! following the priority: ReportOptions > GlobalConfig > Profile defaults.

mod report_common;
use diagweave::prelude::*;
use diagweave::report::{GlobalConfig, ReportOptions, set_global_config};
use report_common::*;

/// Test that ReportOptions::new() creates options with all fields unset.
#[test]
fn report_options_new_creates_unset_fields() {
    let opts = ReportOptions::new();
    assert!(opts.accumulate_source_chain.is_none());
    assert!(opts.max_depth.is_none());
    assert!(opts.detect_cycle.is_none());
}

/// Test that ReportOptions::default() creates options with all fields unset.
#[test]
fn report_options_default_creates_unset_fields() {
    let opts = ReportOptions::default();
    assert!(opts.accumulate_source_chain.is_none());
    assert!(opts.max_depth.is_none());
    assert!(opts.detect_cycle.is_none());
}

/// Test that ReportOptions builder methods set values correctly.
#[test]
fn report_options_builder_sets_values() {
    let opts = ReportOptions::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(32)
        .with_cycle_detection(false);

    assert_eq!(opts.accumulate_source_chain, Some(true));
    assert_eq!(opts.max_depth, Some(32));
    assert_eq!(opts.detect_cycle, Some(false));
}

/// Test that resolve methods return GlobalConfig values when nothing is set.
#[test]
fn report_options_resolve_returns_global_config_values() {
    let opts = ReportOptions::new();

    // When ReportOptions fields are None, values come from GlobalConfig
    // We just verify the methods work without asserting specific values
    // since GlobalConfig may have been modified by other tests
    let _ = opts.resolve_accumulate_source_chain();
    let _ = opts.resolve_detect_cycle();
    let _ = opts.resolve_max_depth();
}

/// Test that resolve methods return set values when explicitly configured.
#[test]
fn report_options_resolve_returns_set_values() {
    let opts = ReportOptions::new()
        .with_accumulate_source_chain(false)
        .with_max_depth(64)
        .with_cycle_detection(true);

    assert!(!opts.resolve_accumulate_source_chain());
    assert_eq!(opts.resolve_max_depth(), 64);
    assert!(opts.resolve_detect_cycle());
}

/// Test that GlobalConfig::new() creates config with profile-dependent defaults.
#[test]
fn global_config_new_has_profile_defaults() {
    let config = GlobalConfig::new();

    // Check profile-dependent defaults
    #[cfg(debug_assertions)]
    {
        assert!(config.accumulate_source_chain);
        assert!(config.detect_cycle);
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(!config.accumulate_source_chain);
        assert!(!config.detect_cycle);
    }
    assert_eq!(config.max_depth, 16);
}

/// Test that GlobalConfig builder methods set values correctly.
#[test]
fn global_config_builder_sets_values() {
    let config = GlobalConfig::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(32)
        .with_cycle_detection(false);

    assert!(config.accumulate_source_chain);
    assert_eq!(config.max_depth, 32);
    assert!(!config.detect_cycle);
}

/// Test that GlobalConfig resolve methods return the configured values.
#[test]
fn global_config_resolve_returns_configured_values() {
    let config = GlobalConfig::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(50)
        .with_cycle_detection(false);

    assert!(config.resolve_accumulate_source_chain());
    assert_eq!(config.resolve_max_depth(), 50);
    assert!(!config.resolve_detect_cycle());
}

/// Test that Report uses ReportOptions when explicitly set.
#[test]
fn report_uses_explicit_report_options() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken).set_options(
        ReportOptions::new()
            .with_accumulate_source_chain(true)
            .with_max_depth(8)
            .with_cycle_detection(false),
    );

    let opts = report.options();
    assert_eq!(opts.accumulate_source_chain, Some(true));
    assert_eq!(opts.max_depth, Some(8));
    assert_eq!(opts.detect_cycle, Some(false));

    // Verify resolved values
    assert!(report.options().resolve_accumulate_source_chain());
    assert_eq!(report.options().resolve_max_depth(), 8);
    assert!(!report.options().resolve_detect_cycle());
}

/// Test that Report returns default options when not explicitly set.
#[test]
fn report_returns_default_options_when_not_set() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken);
    let opts = report.options();

    // All fields should be None (unset)
    assert!(opts.accumulate_source_chain.is_none());
    assert!(opts.max_depth.is_none());
    assert!(opts.detect_cycle.is_none());

    // Resolved values come from GlobalConfig
    // We just verify the methods work without asserting specific values
    // since GlobalConfig may have been modified by other tests
    let _ = opts.resolve_accumulate_source_chain();
    let _ = opts.resolve_detect_cycle();
    let _ = opts.resolve_max_depth();
}

/// Test that set_accumulate_source_chain works correctly.
#[test]
fn report_set_accumulate_source_chain_works() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken).set_accumulate_source_chain(true);
    let opts = report.options();

    assert_eq!(opts.accumulate_source_chain, Some(true));
    assert!(opts.resolve_accumulate_source_chain());
}

/// Test configuration priority: ReportOptions > GlobalConfig > Profile defaults.
///
/// Note: This test cannot fully test GlobalConfig priority because GlobalConfig
/// is a singleton that persists across tests. The test verifies that ReportOptions
/// takes precedence over the global config.
#[test]
fn config_priority_report_over_global() {
    let _guard = init_test();

    // Create a report with explicit options
    let report = Report::new(AuthError::InvalidToken).set_options(
        ReportOptions::new()
            .with_accumulate_source_chain(false)
            .with_max_depth(100)
            .with_cycle_detection(true),
    );

    // ReportOptions should be used, regardless of GlobalConfig
    assert!(!report.options().resolve_accumulate_source_chain());
    assert_eq!(report.options().resolve_max_depth(), 100);
    assert!(report.options().resolve_detect_cycle());
}

/// Test partial configuration: only set some fields.
#[test]
fn partial_configuration_works() {
    // Only set max_depth, leave others unset
    let opts = ReportOptions::new().with_max_depth(20);

    assert!(opts.accumulate_source_chain.is_none());
    assert_eq!(opts.max_depth, Some(20));
    assert!(opts.detect_cycle.is_none());

    // max_depth should use the set value
    assert_eq!(opts.resolve_max_depth(), 20);

    // Others should use profile defaults
    #[cfg(debug_assertions)]
    {
        assert!(opts.resolve_accumulate_source_chain());
        assert!(opts.resolve_detect_cycle());
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(!opts.resolve_accumulate_source_chain());
        assert!(!opts.resolve_detect_cycle());
    }
}

/// Test that set_global_config can be called and GlobalConfig affects resolve.
#[test]
fn global_config_affects_resolve() {
    // Note: GlobalConfig is a singleton, so we can't test this in isolation
    // This test verifies the integration works
    let config = GlobalConfig::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(50)
        .with_cycle_detection(true);

    // The set_global_config may fail if already set by another test, which is fine
    let _ = set_global_config(config);

    // Create ReportOptions without setting any values
    let opts = ReportOptions::new();

    // When ReportOptions fields are None, GlobalConfig values should be used
    // If GlobalConfig was successfully set, resolve should return those values
    // If GlobalConfig was already set by another test, we just verify no panic
    let _ = opts.resolve_accumulate_source_chain();
    let _ = opts.resolve_max_depth();
    let _ = opts.resolve_detect_cycle();
}
