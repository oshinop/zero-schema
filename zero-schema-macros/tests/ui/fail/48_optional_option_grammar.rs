use zero_schema_macros::zero;

mod lookalike {
    pub struct Option<T>(pub T);
}

#[zero]
struct Malformed {
    bare: Option,
}

#[zero]
struct Noncanonical {
    value: lookalike::Option<u8>,
}

fn main() {}
