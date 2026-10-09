// A zero-start receiving buffer is only constructible for a root whose all-zero
// span is type-valid. Neither enum below declares a `= 0` discriminant, so both
// `ZERO_IS_VALID` constants are false and the checked constructor fails to compile.
// The `Option<Mode>` fix is in `pass/13_zeroed_buffer_option_fix.rs`.
use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Idle = 1,
    Busy = 2,
}

#[zero(crate = zs)]
pub struct Status {
    mode: Mode,
    count: u32,
}

fn main() {
    let _ = zs::make_zeroed_schema_buffer!(Status);
    let _ = zs::make_zeroed_schema_buffer!(Mode);
}
