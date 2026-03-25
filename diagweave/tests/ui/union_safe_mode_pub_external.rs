use diagweave::union;

#[derive(Debug)]
/// Test AuthError.
pub enum AuthError {
    InvalidToken,
}

impl core::fmt::Display for AuthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "invalid token"),
        }
    }
}

impl std::error::Error for AuthError {}

union! {
    #[union(visibility = "strict")]
    pub enum ApiError = AuthError
}

fn main() {}