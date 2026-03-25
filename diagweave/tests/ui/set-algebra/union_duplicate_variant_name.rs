use diagweave::union;

enum AuthError {}

union! {
    enum ApiError = AuthError | {
        AuthError,
    }
}

fn main() {}