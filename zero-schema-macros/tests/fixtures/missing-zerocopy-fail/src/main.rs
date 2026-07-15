use zero_schema_macros::zero;

#[zero(crate = zs)]
struct MissingDirectZerocopy {
    value: u8,
}

fn main() {}
