use zero_schema_macros::zero;

#[zero]
struct AccessMutCollision {
    access_mut: u8,
}

fn main() {}
