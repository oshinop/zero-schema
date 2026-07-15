#![no_std]
#![allow(dead_code, unused_imports)]

use widestring::{U16CStr, U16Str};
use zero_schema::{Endian, FieldKind, StringEncoding, zero};

#[cfg(target_endian = "little")]
const NATIVE_U16: [u8; 2] = [0x34, 0x12];
#[cfg(target_endian = "big")]
const NATIVE_U16: [u8; 2] = [0x12, 0x34];
#[cfg(target_endian = "little")]
const NATIVE_PREFIX: [u8; 2] = [1, 0];
#[cfg(target_endian = "big")]
const NATIVE_PREFIX: [u8; 2] = [0, 1];

macro_rules! u16str_fixture {
    ($feature:literal, $module:ident, $container:meta, $field:meta, $prefix:expr, $endian:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;

            #[$container]
            pub struct Root<'a> {
                #[$field]
                value: &'a U16Str,
            }

            #[repr(align(2))]
            struct Producer([u8; 6]);

            pub fn instantiate() -> usize {
                Root::SCHEMA_SIZE + Root::SCHEMA_ALIGN + Root::SCHEMA_STRIDE
            }

            pub fn verify() -> bool {
                let producer = Producer([
                    ($prefix)[0],
                    ($prefix)[1],
                    NATIVE_U16[0],
                    NATIVE_U16[1],
                    0,
                    0,
                ]);
                let view = match Root::access(&producer.0) {
                    Ok(view) => view,
                    Err(_) => return false,
                };
                let prefix_matches = match Root::LAYOUT.fields()[0].kind() {
                    FieldKind::String(string) => string
                        .length()
                        .is_some_and(|length| length.endian() == $endian),
                    _ => false,
                };
                prefix_matches && view.value().as_slice() == [0x1234]
            }
        }
    };
}

macro_rules! scalar_fixture {
    ($feature:literal, $module:ident, $container:meta, $bytes:expr, $endian:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;

            #[$container]
            pub struct Root {
                value: u16,
            }

            #[repr(align(2))]
            struct Producer([u8; 2]);

            pub fn instantiate() -> usize {
                Root::SCHEMA_SIZE + Root::SCHEMA_ALIGN + Root::SCHEMA_STRIDE
            }

            pub fn verify() -> bool {
                let producer = Producer($bytes);
                let view = match Root::access(&producer.0) {
                    Ok(view) => view,
                    Err(_) => return false,
                };
                let endian_matches = matches!(
                    Root::LAYOUT.fields()[0].kind(),
                    FieldKind::Primitive { endian, .. } if endian == $endian
                );
                endian_matches && view.value() == 0x1234
            }
        }
    };
}

#[cfg(feature = "u16cstr-native")]
mod u16cstr_native {
    use super::*;

    #[zero]
    pub struct Root<'a> {
        #[zero(capacity = 2)]
        value: &'a U16CStr,
    }

    #[repr(align(2))]
    struct Producer([u8; 4]);

    pub fn instantiate() -> usize {
        Root::SCHEMA_SIZE + Root::SCHEMA_ALIGN + Root::SCHEMA_STRIDE
    }

    pub fn verify() -> bool {
        let producer = Producer([NATIVE_U16[0], NATIVE_U16[1], 0, 0]);
        let view = match Root::access(&producer.0) {
            Ok(view) => view,
            Err(_) => return false,
        };
        let units_are_native = matches!(
            Root::LAYOUT.fields()[0].kind(),
            FieldKind::String(string)
                if string.encoding() == StringEncoding::U16C && string.length().is_none()
        );
        units_are_native && view.value().as_slice() == [0x1234]
    }
}

macro_rules! reject_u16cstr_endian {
    ($feature:literal, $module:ident, $endian:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use super::*;

            #[zero]
            struct Root<'a> {
                #[zero(capacity = 2, endian = $endian)]
                value: &'a U16CStr,
            }
        }
    };
}

u16str_fixture!(
    "u16str-prefix-native",
    u16str_prefix_native,
    zero(),
    zero(capacity = 2, len_type = u16, endian = "native"),
    NATIVE_PREFIX,
    Endian::Native
);
u16str_fixture!(
    "u16str-prefix-little",
    u16str_prefix_little,
    zero(),
    zero(capacity = 2, len_type = u16, endian = "little"),
    [1, 0],
    Endian::Little
);
u16str_fixture!(
    "u16str-prefix-big",
    u16str_prefix_big,
    zero(),
    zero(capacity = 2, len_type = u16, endian = "big"),
    [0, 1],
    Endian::Big
);
u16str_fixture!(
    "u16str-prefix-inherited-little",
    u16str_prefix_inherited_little,
    zero(endian = "little"),
    zero(capacity = 2, len_type = u16),
    [1, 0],
    Endian::Little
);
u16str_fixture!(
    "u16str-prefix-inherited-big",
    u16str_prefix_inherited_big,
    zero(endian = "big"),
    zero(capacity = 2, len_type = u16),
    [0, 1],
    Endian::Big
);

scalar_fixture!(
    "scalar-native",
    scalar_native,
    zero(),
    NATIVE_U16,
    Endian::Native
);
scalar_fixture!(
    "scalar-little",
    scalar_little,
    zero(endian = "little"),
    [0x34, 0x12],
    Endian::Little
);
scalar_fixture!(
    "scalar-big",
    scalar_big,
    zero(endian = "big"),
    [0x12, 0x34],
    Endian::Big
);

reject_u16cstr_endian!(
    "u16cstr-endian-native-reject",
    u16cstr_endian_native_reject,
    "native"
);
reject_u16cstr_endian!(
    "u16cstr-endian-little-reject",
    u16cstr_endian_little_reject,
    "little"
);
reject_u16cstr_endian!(
    "u16cstr-endian-big-reject",
    u16cstr_endian_big_reject,
    "big"
);

pub fn feature_gate() -> usize {
    let size = 0;
    #[cfg(feature = "u16str-prefix-native")]
    let size = size + u16str_prefix_native::instantiate();
    #[cfg(feature = "u16str-prefix-little")]
    let size = size + u16str_prefix_little::instantiate();
    #[cfg(feature = "u16str-prefix-big")]
    let size = size + u16str_prefix_big::instantiate();
    #[cfg(feature = "u16str-prefix-inherited-little")]
    let size = size + u16str_prefix_inherited_little::instantiate();
    #[cfg(feature = "u16str-prefix-inherited-big")]
    let size = size + u16str_prefix_inherited_big::instantiate();
    #[cfg(feature = "scalar-native")]
    let size = size + scalar_native::instantiate();
    #[cfg(feature = "scalar-little")]
    let size = size + scalar_little::instantiate();
    #[cfg(feature = "scalar-big")]
    let size = size + scalar_big::instantiate();
    #[cfg(feature = "u16cstr-native")]
    let size = size + u16cstr_native::instantiate();
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_fixtures_access_native_units_and_declared_endian_prefixes() {
        #[cfg(feature = "u16str-prefix-native")]
        assert!(u16str_prefix_native::verify());
        #[cfg(feature = "u16str-prefix-little")]
        assert!(u16str_prefix_little::verify());
        #[cfg(feature = "u16str-prefix-big")]
        assert!(u16str_prefix_big::verify());
        #[cfg(feature = "u16str-prefix-inherited-little")]
        assert!(u16str_prefix_inherited_little::verify());
        #[cfg(feature = "u16str-prefix-inherited-big")]
        assert!(u16str_prefix_inherited_big::verify());
        #[cfg(feature = "scalar-native")]
        assert!(scalar_native::verify());
        #[cfg(feature = "scalar-little")]
        assert!(scalar_little::verify());
        #[cfg(feature = "scalar-big")]
        assert!(scalar_big::verify());
        #[cfg(feature = "u16cstr-native")]
        assert!(u16cstr_native::verify());
    }
}
