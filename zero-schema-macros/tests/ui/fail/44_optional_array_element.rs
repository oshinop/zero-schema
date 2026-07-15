use zero_schema_macros::zero;

#[zero]
struct Invalid {
    values: [Option<Missing>; 2],
}

fn main() {}
