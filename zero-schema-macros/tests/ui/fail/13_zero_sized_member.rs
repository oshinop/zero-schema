use zero_schema_macros::zero;

#[zero]
struct ZeroSizedMember {
    marker: (),
}

fn main() {}
