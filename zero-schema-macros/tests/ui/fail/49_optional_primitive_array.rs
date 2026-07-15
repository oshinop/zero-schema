use zero_schema_macros::zero;

#[zero]
struct PrimitiveArray {
    value: Option<[u8; 2]>,
}

#[zero]
struct BoolArray {
    value: Option<[bool; 2]>,
}

fn main() {}
