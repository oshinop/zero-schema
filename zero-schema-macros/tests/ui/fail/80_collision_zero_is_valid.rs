use zero_schema_macros::zero;

#[zero]
struct RecordCollision {
    ZERO_IS_VALID: u8,
}

#[zero]
#[repr(u8)]
enum EnumCollision {
    ZERO_IS_VALID = 0,
}

fn main() {}
