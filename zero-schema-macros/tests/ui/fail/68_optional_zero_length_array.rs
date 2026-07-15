use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Required {
    One = 1,
}

#[zero]
struct Invalid {
    values: Option<[Required; 0]>,
}

fn main() {}
