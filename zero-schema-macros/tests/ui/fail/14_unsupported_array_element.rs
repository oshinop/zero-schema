use zero_schema_macros::zero;

#[zero]
struct UnsupportedArray<'a> {
    values: [&'a str; 2],
}

fn main() {}
