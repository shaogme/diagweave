#[derive(Debug, diagweave::Error)]
enum ApiError {
    #[display("invalid id: {0}")]
    InvalidId(u64),
    #[display(transparent)]
    Io(#[from] std::io::Error),
}

#[test]
fn derive_error_supports_from_and_diag() {
    let err: ApiError = std::io::Error::other("socket closed").into();
    assert_eq!(err.to_string(), "socket closed");
    assert!(err.source().is_some());

    let report = ApiError::InvalidId(9).diag();
    assert_eq!(report.to_string(), "invalid id: 9");
}
