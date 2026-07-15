use zero_schema_macros::zero;

#[zero]
struct Arithmetic<const N: usize> {
    values: [u32; N + 1],
}

fn main() {}
