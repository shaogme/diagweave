mod report_common;
use diagweave::prelude::*;
use diagweave::report::Attachment;
use diagweave::report::{CauseCollectOptions, ErrorCode, ErrorCodeIntError};
use report_common::*;
use std::error::Error;

fn fail_auth() -> Result<(), AuthError> {
    Err(AuthError::InvalidToken)
}

#[test]
fn source_errors_iterator_preserves_long_attached_chain() {
    let _guard = init_test();

    #[derive(Debug)]
    struct ChainLinkError {
        idx: usize,
        source: Option<Box<dyn Error + Send + Sync + 'static>>,
    }

    impl std::fmt::Display for ChainLinkError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "link {}", self.idx)
        }
    }

    impl Error for ChainLinkError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref().map(|v| v as &(dyn Error + 'static))
        }
    }

    fn build_chain_error(depth: usize) -> ChainLinkError {
        let mut source: Option<Box<dyn Error + Send + Sync + 'static>> = None;
        for idx in (0..depth).rev() {
            source = Some(Box::new(ChainLinkError { idx, source }));
        }
        let root = source.expect("chain should be created");
        let root: Box<ChainLinkError> = root
            .downcast::<ChainLinkError>()
            .expect("root type should be ChainLinkError");
        *root
    }

    let report = Report::new(ApiError::Unauthorized).with_diag_src_err(build_chain_error(20));

    let collected: Vec<(String, usize)> = report
        .diag_source_errors()
        .map(|err| (err.message, err.depth))
        .collect();

    assert_eq!(collected.len(), 20);
    assert_eq!(
        collected.first().map(|(msg, _)| msg.as_str()),
        Some("link 0")
    );
    assert_eq!(
        collected.last().map(|(msg, _)| msg.as_str()),
        Some("link 19")
    );
    assert_eq!(collected.last().map(|(_, depth)| *depth), Some(19));
}

#[test]
fn wrap_preserves_deep_source_chains() {
    let _guard = init_test();

    let report = (0..18).fold(Report::new(ApiError::Unauthorized), |report, idx| {
        report.map_err(|_| ApiError::Wrapped {
            code: 500 + idx as u16,
        })
    });

    let mut iter = report.iter_origin_src_ext(CauseCollectOptions {
        max_depth: 32,
        detect_cycle: true,
    });
    let collected: Vec<String> = iter.by_ref().map(|err| err.message).collect();
    let iter_state = iter.state();

    assert!(collected.len() > 16);
    assert!(!iter_state.truncated);
}

#[test]
fn result_inspect_ext_reads_report_fields() {
    let _guard = init_test();

    let err: Result<(), Report<AuthError, HasSeverity>> = fail_auth().diag(|r| {
        r.with_error_code("AUTH.INVALID_TOKEN")
            .with_severity(Severity::Error)
            .with_category("auth")
            .with_retryable(false)
            .with_ctx("request_id", "req-inspect")
    });

    assert_eq!(
        err.report_error_code().map(ToString::to_string),
        Some("AUTH.INVALID_TOKEN".to_owned())
    );
    assert_eq!(err.report_severity(), Some(Severity::Error));
    assert_eq!(err.report_severity(), Some(Severity::Error));
    assert_eq!(err.report_category(), Some("auth"));
    assert_eq!(err.report_retryable(), Some(false));
    assert_eq!(
        err.report_attachments()
            .map(|items: &[Attachment]| items.len()),
        Some(0)
    );

    let ok: Result<(), Report<AuthError>> = Ok(());
    assert!(ok.report_ref().is_none());
}

#[test]
fn pretty_options_can_hide_specific_sections() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_ctx("request_id", "req-sec-1");
    let opts = ReportRenderOptions {
        show_empty_sections: true,
        show_governance_section: false,
        show_context_section: false,
        show_attachments_section: false,
        show_cause_chains_section: false,
        ..ReportRenderOptions::default()
    };
    let pretty = report.render(Pretty::new(opts)).to_string();
    assert!(!pretty.contains("Governance:"));
    assert!(!pretty.contains("Context:"));
    assert!(!pretty.contains("Attachments:"));
    assert!(!pretty.contains("Display Causes:"));
    assert!(!pretty.contains("Source Errors:"));
}

#[test]
fn error_code_accepts_try_into_integers_and_falls_back_to_string() {
    let _guard = init_test();

    assert_eq!(ErrorCode::from(42usize), ErrorCode::Integer(42));
    assert_eq!(ErrorCode::from(-7isize), ErrorCode::Integer(-7));

    let too_large = u128::MAX;
    let code = ErrorCode::from(too_large);
    assert_eq!(code, ErrorCode::String(too_large.to_string().into()));
    assert_eq!(code.to_string(), too_large.to_string());
}

#[test]
fn error_code_supports_try_into_integer_and_into_string() {
    let _guard = init_test();

    let v: i32 = ErrorCode::from("42")
        .try_into()
        .expect("string integer should parse");
    assert_eq!(v, 42);

    let by_ref: u64 = (&ErrorCode::from(9u8))
        .try_into()
        .expect("integer variant should convert");
    assert_eq!(by_ref, 9);

    let out_of_range: Result<u8, ErrorCodeIntError> = ErrorCode::from(300i32).try_into();
    assert_eq!(out_of_range, Err(ErrorCodeIntError::OutOfRange));

    let invalid: Result<i64, ErrorCodeIntError> = ErrorCode::from("E_AUTH").try_into();
    assert_eq!(invalid, Err(ErrorCodeIntError::InvalidIntegerString));

    let s_from_into: String = ErrorCode::from(123u16).into();
    assert_eq!(s_from_into, "123");

    let s_from_to_string = ErrorCode::from("AUTH.INVALID_TOKEN").to_string();
    assert_eq!(s_from_to_string, "AUTH.INVALID_TOKEN");
}

#[test]
fn report_is_send_sync_when_inner_error_is_send_sync() {
    #[derive(Debug)]
    struct SendSyncErr;

    impl std::fmt::Display for SendSyncErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "send-sync err")
        }
    }

    impl Error for SendSyncErr {}

    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<Report<SendSyncErr>>();
}

#[test]
fn severity_parsing_is_explicit_and_rejects_unknown_values() {
    let _guard = init_test();

    assert_eq!(
        Severity::parse("warning").expect("warning alias should parse"),
        Severity::Warn
    );

    let err = Severity::parse("urgent").expect_err("unknown level should fail parsing");
    assert_eq!(err.invalid_value(), "urgent");
}
