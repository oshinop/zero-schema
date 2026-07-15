use zero_schema_macros::zero;

#[zero]
struct Invalid {
    #[zero(capacity = 4)]
    value: Option<Missing>,
}

fn main() {}
