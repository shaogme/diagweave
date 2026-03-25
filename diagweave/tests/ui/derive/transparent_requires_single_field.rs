use diagweave::Error;

#[derive(Debug, Error)]
enum BadTransparent {
    #[display(transparent)]
    TooMany(std::io::Error, u32),
}

fn main() {}