use zero_schema_macros::zero;

#[zero(unknown = "value")]
struct UnknownOption {
    value: u8,
}

fn main() {}
