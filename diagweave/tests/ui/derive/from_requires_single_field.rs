use diagweave::Error;

#[derive(Debug, Error)]
enum BadFrom {
    #[display("bad from")]
    Multi(#[from] std::io::Error, u32),
}

fn main() {}