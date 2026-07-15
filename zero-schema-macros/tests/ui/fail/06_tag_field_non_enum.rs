use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    Value = 1,
}

#[zero(crate = zs)]
enum Payload {
    #[zero(tag = Kind::Value)]
    Value,
}

#[zero(crate = zs)]
struct NonEnumTagField {
    tag: u8,
    #[zero(tag_field = tag)]
    payload: Payload,
}

fn main() {}
