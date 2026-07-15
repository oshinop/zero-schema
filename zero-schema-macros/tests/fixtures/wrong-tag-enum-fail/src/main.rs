#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ExpectedKind {
    Value = 1,
}

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ActualKind {
    Value = 1,
}

#[zero(crate = zs)]
enum Payload {
    #[zero(tag = ExpectedKind::Value)]
    Value,
}

#[zero(crate = zs)]
struct WrongEnumTagField {
    tag: ActualKind,
    #[zero(tag_field = tag)]
    payload: Payload,
}

fn main() {}
