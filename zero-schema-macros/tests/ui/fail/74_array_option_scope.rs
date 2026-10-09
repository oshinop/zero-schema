#[allow(unused_imports)]
use core::ffi::CStr;

use zero_schema_macros::zero;

type U16CStr = zs::__private::U16CStr;

// The field's string options apply to every element, so each option keeps the
// same validity it has on a single string field.
#[zero(crate = zs)]
struct NarrowPrefix<'a> {
    #[zero(capacity = 4, len_type = u8)]
    codes: [&'a CStr; 2],
}

#[zero(crate = zs)]
struct WideEndian<'a> {
    #[zero(capacity = 4, endian = "little")]
    names: [&'a U16CStr; 2],
}

#[zero(crate = zs)]
struct EmptyTerminated<'a> {
    #[zero(capacity = 0)]
    names: [&'a U16CStr; 2],
}

// Non-string arrays still reject string options, however deeply nested.
#[zero(crate = zs)]
struct NumericCapacity {
    #[zero(capacity = 4)]
    grid: [[u8; 2]; 2],
}

#[zero(crate = zs)]
struct NumericEndian {
    #[zero(endian = "big")]
    grid: [[u16; 2]; 2],
}

fn main() {}
