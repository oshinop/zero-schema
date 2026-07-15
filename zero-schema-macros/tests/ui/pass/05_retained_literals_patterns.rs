#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs, endian = "big")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum PatternKind {
    Zero = 0,
    Hex = 0x2a,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Retained<'a> {
    #[zero(capacity = 4, len_type = u16, endian = "little")]
    pub text: &'a str,
    pub kind: PatternKind,
}

fn main() {
    let retained = Retained {
        text: "ok",
        kind: PatternKind::Hex,
    };
    let PatternKind::Hex = retained.kind else {
        panic!("retained enum pattern changed");
    };
    assert_eq!(retained, Retained { text: "ok", kind: PatternKind::Hex });
    let _ = Retained::<'static>::SCHEMA_STRIDE;
}
