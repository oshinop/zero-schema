#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Idle = 1,
    Busy = 2,
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Low = 0,
    High = 1,
}

// The enum has no zero discriminant, so a bare `Mode` field makes the root
// zero-invalid (see `fail/79_zeroed_buffer_zero_invalid_enum.rs`).
#[zero(crate = zs)]
pub struct Rejected {
    mode: Mode,
}

// `Option<Mode>` is absent when all zero, so this root is zero-valid.
#[zero(crate = zs)]
pub struct Accepted {
    mode: Option<Mode>,
    level: Level,
    count: u32,
}

const _: () = assert!(!Rejected::ZERO_IS_VALID);
const _: () = assert!(Accepted::ZERO_IS_VALID);

pub fn exercise() {
    let slot = zs::make_zeroed_schema_buffer!(Accepted);
    let view = Accepted::access(slot.as_bytes()).expect("zero-start root is type-valid");
    assert_eq!((view.mode(), view.level(), view.count()), (None, Level::Low, 0));
}

fn main() {
    exercise();
}
