#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    Empty = 1,
}

#[zero(crate = zs)]
enum Payload {
    #[zero(tag = Kind::Empty)]
    Empty,
}

#[zero(crate = zs)]
struct MissingTagField {
    kind: Kind,
    payload: Payload,
}

fn main() {}
