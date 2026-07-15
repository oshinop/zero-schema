use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<bool>,
}

fn main() {}
