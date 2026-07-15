use zero_schema_macros::zero;

#[zero]
struct Invalid {
    core_bool: Option<::core::primitive::bool>,
    std_number: Option<std::primitive::u8>,
    std_bool_array: Option<[std::primitive::bool; 2]>,
}

fn main() {}
