use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<[std::primitive::bool; 2]>,
}

fn main() {}
