#![deny(private_interfaces)]

use diagweave::union;

mod inner {
    #[derive(Debug)]
    pub(crate) enum InternalError {
        Oops,
    }

    impl core::fmt::Display for InternalError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Self::Oops => write!(f, "oops"),
            }
        }
    }

    impl std::error::Error for InternalError {}
}

use inner::InternalError;

union! {
    pub enum ApiError = InternalError
}

fn main() {}