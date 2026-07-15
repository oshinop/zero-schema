use zero_schema_macros::zero;

#[zero]
struct SchemaSizeCollision {
    SCHEMA_SIZE: u8,
}

fn main() {}
