use diagweave::union;

union! {
    enum BadTransparent = {
        #[display(transparent)]
        NotTuple { code: u32 },
    }
}

fn main() {}
