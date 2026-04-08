use diagweave::Error;

#[derive(Debug, Error)]
enum DemoError {
    #[display("not found: {id}")]
    NotFound { id: u64 },
    #[display(transparent)]
    Io(#[from] std::io::Error),
    #[display("upstream failed: {0}")]
    Upstream(#[source] std::io::Error),
}

#[derive(Debug, Error)]
#[display("client error code={code}")]
struct ClientError {
    code: u16,
}

#[test]
fn derive_error_template_and_transparent_display_work() {
    let msg = DemoError::NotFound { id: 7 };
    assert_eq!(msg.to_string(), "not found: 7");

    let io: DemoError = std::io::Error::other("socket closed").into();
    assert_eq!(io.to_string(), "socket closed");
}

#[test]
fn derive_source_errors_and_diag_work() {
    let up = DemoError::Upstream(std::io::Error::other("db down"));
    let src = up.source().expect("source exists");
    assert_eq!(src.to_string(), "db down");

    let report = ClientError { code: 403 }.to_report();
    assert_eq!(report.into_inner().to_string(), "client error code=403");
}

#[test]
fn derive_direct_diag_on_client_error() {
    // Directly call diag on a ClientError instance (inherent method provided by macro)
    let report = ClientError { code: 403 }.diag(|r| r.attach_note("note"));
    // Ensure we got a report and inner error renders as expected
    assert_eq!(report.inner().to_string(), "client error code=403");
}
