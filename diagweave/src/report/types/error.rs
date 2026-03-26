use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use core::convert::TryFrom;
use core::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Display for Severity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        };
        write!(f, "{label}")
    }
}

impl From<&str> for Severity {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            "fatal" | "critical" => Self::Fatal,
            _ => Self::Error,
        }
    }
}

impl From<String> for Severity {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<Cow<'static, str>> for Severity {
    fn from(value: Cow<'static, str>) -> Self {
        match value.to_lowercase().as_str() {
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            "fatal" | "critical" => Self::Fatal,
            _ => Self::Error,
        }
    }
}

impl From<Severity> for Cow<'static, str> {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Debug => "debug".into(),
            Severity::Info => "info".into(),
            Severity::Warn => "warn".into(),
            Severity::Error => "error".into(),
            Severity::Fatal => "fatal".into(),
        }
    }
}

/// An error code that can be either an integer or a string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(untagged))]
pub enum ErrorCode {
    /// An integer error code.
    Integer(i64),
    /// A string error code.
    String(Cow<'static, str>),
}

/// Error type for converting an [`ErrorCode`] to an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCodeIntError {
    /// The string representation of the error code is not a valid integer.
    InvalidIntegerString,
    /// The integer is out of range for the target type.
    OutOfRange,
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(v) => write!(f, "{v}"),
            Self::String(v) => write!(f, "{v}"),
        }
    }
}

impl From<ErrorCode> for String {
    fn from(value: ErrorCode) -> Self {
        match value {
            ErrorCode::Integer(v) => v.to_string(),
            ErrorCode::String(v) => v.to_string(),
        }
    }
}

impl From<&ErrorCode> for String {
    fn from(value: &ErrorCode) -> Self {
        value.to_string()
    }
}

macro_rules! impl_error_code_from_integer_try_into_i64 {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for ErrorCode {
                fn from(v: $ty) -> Self {
                    match i64::try_from(v) {
                        Ok(value) => Self::Integer(value),
                        Err(_) => Self::String(v.to_string().into()),
                    }
                }
            }
        )*
    };
}

impl_error_code_from_integer_try_into_i64!(
    i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, i128, u128,
);

macro_rules! impl_try_from_error_code_for_signed_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TryFrom<ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<i128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }

            impl TryFrom<&ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: &ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(*v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<i128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }
        )*
    };
}

macro_rules! impl_try_from_error_code_for_unsigned_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TryFrom<ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<u128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }

            impl TryFrom<&ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: &ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(*v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<u128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }
        )*
    };
}

impl_try_from_error_code_for_signed_int!(i8, i16, i32, i64, isize, i128);
impl_try_from_error_code_for_unsigned_int!(u8, u16, u32, u64, usize, u128);

impl From<String> for ErrorCode {
    fn from(v: String) -> Self {
        Self::String(v.into())
    }
}

impl From<&'static str> for ErrorCode {
    fn from(v: &'static str) -> Self {
        Self::String(v.into())
    }
}
