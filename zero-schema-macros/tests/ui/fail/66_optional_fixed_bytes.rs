use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<&'static [u8; 2]>,
}

fn main() {}
