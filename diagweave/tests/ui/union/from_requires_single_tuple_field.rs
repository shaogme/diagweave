use diagweave::union;

union! {
    enum BadFrom = {
        #[from]
        NotTuple { code: u32 },
    }
}

fn main() {}
