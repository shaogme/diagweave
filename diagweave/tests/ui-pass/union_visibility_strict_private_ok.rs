use diagweave::union;

#[derive(Debug)]
enum InternalError {
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

union! {
    enum LocalError = InternalError
}

fn main() {
    let _ = LocalError::from(InternalError::Oops);
}