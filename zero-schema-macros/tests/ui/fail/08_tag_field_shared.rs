use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    First = 1,
    Second = 2,
}

#[zero(crate = zs)]
enum FirstPayload {
    #[zero(tag = Kind::First)]
    First,
}

#[zero(crate = zs)]
enum SecondPayload {
    #[zero(tag = Kind::Second)]
    Second,
}

#[zero(crate = zs)]
struct SharedTagField {
    tag: Kind,
    #[zero(tag_field = tag)]
    first: FirstPayload,
    #[zero(tag_field = tag)]
    second: SecondPayload,
}

fn main() {}
