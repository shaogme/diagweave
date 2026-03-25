use diagweave::set;

set! {
    AuthError = {
        #[display("user {uid} token is invalid")]
        InvalidToken { user_id: u64 },
    }
}

fn main() {}
