// `clear()` exists only where an all-zero span is certainly a valid value of the
// record, so each call below fails at the call site, under `cargo check` too. The
// definitions themselves compile. The `Option<Mode>` fix is in
// `pass/14_clear_zero_valid_option_fix.rs`.
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
pub enum Kind {
    Empty = 0,
    Data = 1,
}

#[zero(crate = zs)]
pub struct Status {
    mode: Mode,
    count: u32,
}

#[zero(crate = zs)]
pub struct Parent {
    status: Status,
    list: [Status; 2],
}

// The zero tag selects the unit variant, so `ZERO_IS_VALID` is true, but another
// variant is zero-invalid and a type-level check cannot tell which one the tag picks.
#[zero(crate = zs)]
pub enum UnitAtZero {
    #[zero(tag = Kind::Empty)]
    Empty,
    #[zero(tag = Kind::Data)]
    Data(Status),
}

#[zero(crate = zs)]
pub struct Tagged {
    kind: Kind,
    #[zero(tag_field = kind)]
    payload: UnitAtZero,
}

const _: () = assert!(Tagged::ZERO_IS_VALID);

// The tag enum has a zero value that no variant claims, so the zero span is invalid.
#[zero(crate = zs)]
pub struct Plain {
    value: u32,
}

#[zero(crate = zs)]
pub enum Unclaimed {
    #[zero(tag = Kind::Data)]
    Data(Plain),
}

#[zero(crate = zs)]
pub struct TaggedUnclaimed {
    kind: Kind,
    #[zero(tag_field = kind)]
    payload: Unclaimed,
}

const _: () = assert!(!TaggedUnclaimed::ZERO_IS_VALID);

pub fn clear_status(status: &mut StatusMut<'_>) {
    status.clear();
}

pub fn clear_nested(parent: &mut ParentMut<'_>) {
    parent.clear();
    parent.status_mut().clear();
    parent.list_mut().get_mut(0).expect("first element").clear();
}

pub fn clear_tagged(tagged: &mut TaggedMut<'_>) {
    tagged.clear();
}

pub fn clear_unclaimed(tagged: &mut TaggedUnclaimedMut<'_>) {
    tagged.clear();
}

fn main() {}
