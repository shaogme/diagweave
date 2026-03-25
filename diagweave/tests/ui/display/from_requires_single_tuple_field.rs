use diagweave::set;

set! {
    BadFrom = {
        #[from]
        NotTuple { code: u32 },
    }
}

fn main() {}