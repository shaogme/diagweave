use diagweave::set;

mod custom_runtime {
    /// A simple wrapper for testing.
    pub struct Bag<T>(T);

    impl<T> Bag<T> {
        /// Creates a new Bag.
        pub fn new(inner: T) -> Self {
            Self(inner)
        }

        /// Returns the inner value.
        pub fn into_inner(self) -> T {
            self.0
        }
    }
}

set! {
    AuthError = {
        #[display("Invalid authentication token")]
        InvalidToken,
        #[display("Permission denied for user {id}")]
        PermissionDenied { id: u32 },
    }

    ApiError = AuthError | {
        #[display("Rate limited; retry after {retry_after}s")]
        RateLimited { retry_after: u64 },
        #[display("Template escape: {{ok}} and id={id}")]
        EscapedTemplate { id: u32 },
    }
}

set! {
    #[diagweave(constructor_prefix = "new")]
    PrefixError = {
        #[display("Invalid token for {user_id}")]
        InvalidToken { user_id: u64 },
    }
}

set! {
    WrapperError = {
        #[from]
        #[display(transparent)]
        Io(std::io::Error),
        #[display("config parse failed: {0}")]
        Config(&'static str),
    }
}

set! {
    #[diagweave(report_path = "crate::custom_runtime::Bag")]
    CustomPathError = {
        #[display("Custom runtime path works")]
        Works,
    }
}

#[test]
fn converts_subset_into_union() {
    let auth = AuthError::PermissionDenied { id: 42 };
    let api: ApiError = auth.into();
    match api {
        ApiError::PermissionDenied { id } => assert_eq!(id, 42),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn keeps_inline_variants() {
    let api = ApiError::RateLimited { retry_after: 5 };
    match api {
        ApiError::RateLimited { retry_after } => assert_eq!(retry_after, 5),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn repeated_conversion_stays_correct() {
    for i in 0..10_000u32 {
        let auth = if i % 2 == 0 {
            AuthError::InvalidToken
        } else {
            AuthError::PermissionDenied { id: i }
        };
        let api: ApiError = auth.into();
        match api {
            ApiError::InvalidToken if i % 2 == 0 => {}
            ApiError::PermissionDenied { id } if i % 2 == 1 && id == i => {}
            _ => panic!("unexpected conversion result"),
        }
    }
}

#[test]
fn display_attribute_renders_structured_fields() {
    let auth = AuthError::PermissionDenied { id: 7 };
    let api = ApiError::RateLimited { retry_after: 3 };
    assert_eq!(auth.to_string(), "Permission denied for user 7");
    assert_eq!(api.to_string(), "Rate limited; retry after 3s");
    let escaped = ApiError::EscapedTemplate { id: 9 };
    assert_eq!(escaped.to_string(), "Template escape: {ok} and id=9");
}

#[test]
fn generated_variant_constructors_work() {
    let unit = AuthError::invalid_token();
    let named = AuthError::permission_denied(100);
    let inline = ApiError::rate_limited(11);
    let unit_report = AuthError::invalid_token_report();
    let named_report = AuthError::permission_denied_report(200);

    assert_eq!(unit.to_string(), "Invalid authentication token");
    assert_eq!(named.to_string(), "Permission denied for user 100");
    assert_eq!(inline.to_string(), "Rate limited; retry after 11s");
    assert!(matches!(unit_report.into_inner(), AuthError::InvalidToken));
    assert!(matches!(
        named_report.into_inner(),
        AuthError::PermissionDenied { id: 200 }
    ));
}

#[test]
fn generated_report_constructor_supports_custom_report_path() {
    let report = CustomPathError::works_report();
    assert!(matches!(report.into_inner(), CustomPathError::Works));
}

#[test]
fn generated_constructors_support_prefix_configuration() {
    let err = PrefixError::new_invalid_token(42);
    let report = PrefixError::new_invalid_token_report(88);
    assert_eq!(err.to_string(), "Invalid token for 42");
    assert!(matches!(
        report.into_inner(),
        PrefixError::InvalidToken { user_id: 88 }
    ));
}

#[test]
fn set_enum_provides_diag_helpers() {
    let report = AuthError::InvalidToken.diag();
    assert_eq!(report.to_string(), "Invalid authentication token");
    assert!(AuthError::InvalidToken.source().is_none());
}

#[test]
fn from_and_transparent_display_work_for_wrapper_variants() {
    let err: WrapperError = std::io::Error::other("socket closed").into();
    assert_eq!(err.to_string(), "socket closed");

    let cfg = WrapperError::config("missing field");
    assert_eq!(cfg.to_string(), "config parse failed: missing field");
}
