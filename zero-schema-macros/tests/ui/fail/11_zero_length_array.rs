use zero_schema_macros::zero;

#[zero]
struct EmptyArray {
    values: [u8; 0],
}

fn main() {}
