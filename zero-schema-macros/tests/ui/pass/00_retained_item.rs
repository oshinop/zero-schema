#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs, endian = "little")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Retained {
    pub value: u32,
}

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Kind {
    One = 1,
}

fn main() {
    let retained = Retained { value: 7 };
    assert_eq!(retained.value, 7);
    let _ = Retained::SCHEMA_SIZE;
    let _ = Kind::SCHEMA_ALIGN;
}
