use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<u8>,
}

fn main() {}
