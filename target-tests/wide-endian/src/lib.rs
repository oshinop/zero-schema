#![no_std]
#![allow(unused_imports)]

use widestring::{U16CStr, U16Str};
use zero_schema::ZeroSchema;

macro_rules! fixture {
    ($feature:literal, $module:ident, $container:meta, $field:meta, $view:ident) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;

            #[derive(ZeroSchema)]
            #[$container]
            #[zero(borrow = 'a)]
            struct Root<'a> {
                #[$field]
                value: &'a $view,
            }

            pub fn instantiate() -> usize {
                Root::WIRE_SIZE + Root::LAYOUT.size()
            }
        }
    };
}

fixture!(
    "explicit-little-u16str",
    explicit_little_u16str,
    zero(),
    zero(capacity = 2, endian = "little"),
    U16Str
);
fixture!(
    "explicit-big-u16str",
    explicit_big_u16str,
    zero(),
    zero(capacity = 2, endian = "big"),
    U16Str
);
fixture!(
    "inherited-little-u16str",
    inherited_little_u16str,
    zero(endian = "little"),
    zero(capacity = 2),
    U16Str
);
fixture!(
    "inherited-big-u16str",
    inherited_big_u16str,
    zero(endian = "big"),
    zero(capacity = 2),
    U16Str
);
fixture!(
    "native-over-little-u16str",
    native_over_little_u16str,
    zero(endian = "little"),
    zero(capacity = 2, endian = "native"),
    U16Str
);
fixture!(
    "native-over-big-u16str",
    native_over_big_u16str,
    zero(endian = "big"),
    zero(capacity = 2, endian = "native"),
    U16Str
);
fixture!(
    "explicit-little-u16cstr",
    explicit_little_u16cstr,
    zero(),
    zero(capacity = 2, endian = "little"),
    U16CStr
);
fixture!(
    "explicit-big-u16cstr",
    explicit_big_u16cstr,
    zero(),
    zero(capacity = 2, endian = "big"),
    U16CStr
);
fixture!(
    "inherited-little-u16cstr",
    inherited_little_u16cstr,
    zero(endian = "little"),
    zero(capacity = 2),
    U16CStr
);
fixture!(
    "inherited-big-u16cstr",
    inherited_big_u16cstr,
    zero(endian = "big"),
    zero(capacity = 2),
    U16CStr
);
fixture!(
    "native-over-little-u16cstr",
    native_over_little_u16cstr,
    zero(endian = "little"),
    zero(capacity = 2, endian = "native"),
    U16CStr
);
fixture!(
    "native-over-big-u16cstr",
    native_over_big_u16cstr,
    zero(endian = "big"),
    zero(capacity = 2, endian = "native"),
    U16CStr
);

pub fn feature_gate() -> usize {
    let size = 0;
    #[cfg(feature = "explicit-little-u16str")]
    let size = size + explicit_little_u16str::instantiate();
    #[cfg(feature = "explicit-big-u16str")]
    let size = size + explicit_big_u16str::instantiate();
    #[cfg(feature = "inherited-little-u16str")]
    let size = size + inherited_little_u16str::instantiate();
    #[cfg(feature = "inherited-big-u16str")]
    let size = size + inherited_big_u16str::instantiate();
    #[cfg(feature = "native-over-little-u16str")]
    let size = size + native_over_little_u16str::instantiate();
    #[cfg(feature = "native-over-big-u16str")]
    let size = size + native_over_big_u16str::instantiate();
    #[cfg(feature = "explicit-little-u16cstr")]
    let size = size + explicit_little_u16cstr::instantiate();
    #[cfg(feature = "explicit-big-u16cstr")]
    let size = size + explicit_big_u16cstr::instantiate();
    #[cfg(feature = "inherited-little-u16cstr")]
    let size = size + inherited_little_u16cstr::instantiate();
    #[cfg(feature = "inherited-big-u16cstr")]
    let size = size + inherited_big_u16cstr::instantiate();
    #[cfg(feature = "native-over-little-u16cstr")]
    let size = size + native_over_little_u16cstr::instantiate();
    #[cfg(feature = "native-over-big-u16cstr")]
    let size = size + native_over_big_u16cstr::instantiate();
    size
}
