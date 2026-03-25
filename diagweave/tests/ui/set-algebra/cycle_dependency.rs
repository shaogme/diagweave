use diagweave::set;

set! {
    A = B | {
        AOnly,
    }

    B = A | {
        BOnly,
    }
}

fn main() {}