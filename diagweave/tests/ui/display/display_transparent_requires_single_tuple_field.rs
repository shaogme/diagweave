use diagweave::set;

set! {
    BadTransparent = {
        #[display(transparent)]
        NotTuple { code: u32 },
    }
}

fn main() {}