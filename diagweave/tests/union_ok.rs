use diagweave::union;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Test AuthError.
pub enum AuthError {
    InvalidToken,
}

impl Display for AuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "auth token invalid"),
        }
    }
}

impl Error for AuthError {}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Test DbError.
pub enum DbError {
    ConnectionLost,
}

impl Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionLost => write!(f, "db connection lost"),
        }
    }
}

impl Error for DbError {}

union! {
    #[derive(Clone)]
    pub enum ApiError = AuthError | DbError | {
        #[display("Rate limited for {retry_after_secs}s")]
        RateLimited { retry_after_secs: u64 },
        #[display("Escaped braces {{db}} code={0}")]
        TupleEscaped(u32),
    }
}

#[test]
fn wraps_external_error_types() {
    let auth = AuthError::InvalidToken;
    let api: ApiError = auth.into();
    match api {
        ApiError::AuthError(inner) => assert_eq!(inner, AuthError::InvalidToken),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn keeps_inline_variants() {
    let api = ApiError::RateLimited {
        retry_after_secs: 10,
    };
    match api {
        ApiError::RateLimited { retry_after_secs } => assert_eq!(retry_after_secs, 10),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn converts_second_external_type() {
    let db = DbError::ConnectionLost;
    let api: ApiError = db.into();
    match api {
        ApiError::DbError(inner) => assert_eq!(inner, DbError::ConnectionLost),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn supports_alias_for_external_types() {
    union! {
        enum AliasedError = AuthError as Auth | DbError as Database
    }

    let auth: AliasedError = AuthError::InvalidToken.into();
    match auth {
        AliasedError::Auth(inner) => assert_eq!(inner, AuthError::InvalidToken),
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn union_display_works_for_wrapped_and_inline_variants() {
    let wrapped: ApiError = AuthError::InvalidToken.into();
    let inline = ApiError::RateLimited {
        retry_after_secs: 12,
    };
    let escaped = ApiError::TupleEscaped(88);
    assert_eq!(wrapped.to_string(), "auth token invalid");
    assert_eq!(inline.to_string(), "Rate limited for 12s");
    assert_eq!(escaped.to_string(), "Escaped braces {db} code=88");
    let dbg = format!("{:?}", inline);
    assert!(dbg.contains("RateLimited"));
}
