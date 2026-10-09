use zero_schema_macros::zero;

#[zero]
struct UnsupportedArray<'a> {
    values: [&'a [u8; 2]; 2],
}

fn main() {}
