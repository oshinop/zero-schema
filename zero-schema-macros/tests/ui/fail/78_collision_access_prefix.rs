use zero_schema_macros::zero;

#[zero]
struct AccessPrefixCollision {
    access_prefix: u8,
}

#[zero]
struct AccessMutPrefixCollision {
    access_mut_prefix: u8,
}

fn main() {}
