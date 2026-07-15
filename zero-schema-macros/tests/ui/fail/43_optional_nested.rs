use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<Option<Missing>>,
}

fn main() {}
