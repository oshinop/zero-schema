use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<std::primitive::u16>,
}

fn main() {}
