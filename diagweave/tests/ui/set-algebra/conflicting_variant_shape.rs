use diagweave::set;

set! {
    AuthError = {
        PermissionDenied { id: u32 },
    }

    ApiError = AuthError | {
        PermissionDenied { id: String },
    }
}

fn main() {}