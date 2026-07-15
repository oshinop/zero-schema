use zero_schema_macros::zero;

#[zero(padding = "zero", tail = "zero", validate_with = validate)]
struct RemovedContainerOptions {
    value: u8,
}

fn validate() {}

fn main() {}
