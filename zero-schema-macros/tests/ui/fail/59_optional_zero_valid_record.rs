use zero_schema_macros::zero;

#[zero]
struct ZeroValidRecord {
    count: u8,
    enabled: bool,
}

#[zero]
struct Invalid {
    value: Option<ZeroValidRecord>,
}

fn main() {}
