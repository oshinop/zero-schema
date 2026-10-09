use zero_schema_macros::zero;

#[zero(crate = zs)]
struct InnerEmpty {
    grid: [[u8; 0]; 2],
}

#[zero(crate = zs)]
struct OuterEmpty {
    grid: [[u8; 2]; 0],
}

fn main() {}
