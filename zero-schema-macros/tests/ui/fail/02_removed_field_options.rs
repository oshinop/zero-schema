use zero_schema_macros::zero;

#[zero]
struct RemovedFieldOptions {
    #[zero(range = 1, must_equal = 1)]
    value: u8,
}

fn main() {}
