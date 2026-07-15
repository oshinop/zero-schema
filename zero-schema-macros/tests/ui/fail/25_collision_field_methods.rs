use zero_schema_macros::zero;

#[zero]
struct FieldMethodCollision {
    item: u8,
    item_mut: u8,
}

fn main() {}
