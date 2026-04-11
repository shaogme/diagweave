//! Tests for source error iteration and cause visit APIs.
//!
//! This module contains tests related to:
//! - Cause visit APIs (visit_causes, visit_origin_sources, visit_diag_sources)
//! - Source iteration with cycle detection
//! - Source chain isolation
//! - Sibling handling in source trees

mod report_common;
use diagweave::prelude::*;
use diagweave::report::CauseCollectOptions;
use report_common::*;
use std::error::Error;

#[test]
fn public_cause_visit_apis_are_accessible() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_display_cause("token stale")
        .with_diag_src_err(ApiError::Unauthorized);
    let mut display = Vec::new();
    let display_state = report
        .visit_causes(|cause| {
            display.push(cause.to_string());
            Ok(())
        })
        .expect("display causes");
    let mut origin_source = Vec::new();
    let origin_source_state = report
        .visit_origin_sources(|err| {
            origin_source.push(err.error.to_string());
            Ok(())
        })
        .expect("origin source errors");
    let mut diagnostic_source = Vec::new();
    let diagnostic_source_state = report
        .visit_diag_sources(|err| {
            diagnostic_source.push(err.error.to_string());
            Ok(())
        })
        .expect("diagnostic source errors");

    assert_eq!(display, vec!["token stale".to_owned()]);
    assert!(origin_source.is_empty());
    assert_eq!(diagnostic_source, vec!["api unauthorized".to_owned()]);
    assert!(!display_state.truncated);
    assert!(!display_state.cycle_detected);
    assert!(!origin_source_state.truncated);
    assert!(!origin_source_state.cycle_detected);
    assert!(!diagnostic_source_state.truncated);
    assert!(!diagnostic_source_state.cycle_detected);

    let cycle = Report::new(LoopError)
        .visit_origin_src_ext(
            CauseCollectOptions {
                max_depth: 8,
                detect_cycle: true,
            },
            |_| Ok(()),
        )
        .expect("cycle traversal");
    assert!(cycle.cycle_detected);

    let mut iter = report.iter_diag_srcs_ext(CauseCollectOptions {
        max_depth: 4,
        detect_cycle: true,
    });
    let collected: Vec<String> = iter.by_ref().map(|err| err.error.to_string()).collect();
    let iter_state = iter.state();
    assert_eq!(collected, vec!["api unauthorized".to_owned()]);
    assert!(!iter_state.truncated);
    assert!(!iter_state.cycle_detected);
}

#[test]
fn source_iteration_can_disable_cycle_detection() {
    let _guard = init_test();

    let report = Report::new(LoopError);
    let mut iter = report.iter_origin_src_ext(CauseCollectOptions {
        max_depth: 4,
        detect_cycle: false,
    });
    let collected: Vec<String> = iter.by_ref().map(|err| err.error.to_string()).collect();
    let iter_state = iter.state();

    assert_eq!(
        collected,
        vec![
            "loop error".to_owned(),
            "loop error".to_owned(),
            "loop error".to_owned(),
            "loop error".to_owned(),
        ]
    );
    assert!(iter_state.truncated);
    assert!(!iter_state.cycle_detected);
}

#[test]
fn wrap_keeps_explicit_source_chain_isolated_from_inner_source() {
    let _guard = init_test();

    #[derive(Debug)]
    struct NaturalSourceError;

    impl std::fmt::Display for NaturalSourceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "natural source")
        }
    }

    impl Error for NaturalSourceError {}

    #[derive(Debug)]
    struct SourcefulError;

    impl std::fmt::Display for SourcefulError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "sourceful error")
        }
    }

    impl Error for SourcefulError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&NATURAL_SOURCE)
        }
    }

    static NATURAL_SOURCE: NaturalSourceError = NaturalSourceError;

    let report = Report::new(SourcefulError)
        .with_diag_src_err(ApiError::Unauthorized)
        .map_err(|_| ApiError::Wrapped { code: 500 });

    let messages: Vec<String> = report
        .iter_origin_src_ext(CauseCollectOptions {
            max_depth: 8,
            detect_cycle: true,
        })
        .map(|entry| entry.error.to_string())
        .collect();

    assert!(!messages.iter().any(|message| message == "api unauthorized"));
    assert!(messages.iter().any(|message| message == "natural source"));
}

#[test]
fn source_iteration_keeps_top_level_siblings_at_same_depth() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));
    let collected: Vec<(String, usize)> = report
        .iter_diag_srcs_ext(CauseCollectOptions {
            max_depth: 4,
            detect_cycle: true,
        })
        .map(|err| (err.error.to_string(), err.depth))
        .collect();

    assert_eq!(
        collected,
        vec![
            ("auth invalid token".to_owned(), 0),
            ("network down".to_owned(), 0),
        ]
    );
}

#[test]
fn source_iteration_keeps_siblings_after_truncation() {
    let _guard = init_test();

    let deep_branch = Report::new(AuthError::InvalidToken).map_err(|_| ApiError::Unauthorized);
    let report = Report::new(ApiError::Wrapped { code: 400 })
        .with_diag_src_err(deep_branch)
        .with_diag_src_err(std::io::Error::other("network down"));

    let collected: Vec<String> = report
        .iter_diag_srcs_ext(CauseCollectOptions {
            max_depth: 1,
            detect_cycle: true,
        })
        .map(|err| err.error.to_string())
        .collect();

    assert!(collected.iter().any(|message| message == "network down"));
}

#[test]
fn source_errors_iterator_only_uses_attached_chain() {
    let _guard = init_test();

    #[derive(Debug)]
    struct NaturalSourceError;

    impl std::fmt::Display for NaturalSourceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "natural source")
        }
    }

    impl Error for NaturalSourceError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&NATURAL_SOURCE)
        }
    }

    static NATURAL_SOURCE: NaturalSourceError = NaturalSourceError;

    let report = Report::new(NaturalSourceError).with_diag_src_err(AuthError::InvalidToken);
    let collected: Vec<String> = report
        .diag_source_errors()
        .map(|err| err.error.to_string())
        .collect();

    assert_eq!(collected, vec!["auth invalid token".to_owned()]);
}
