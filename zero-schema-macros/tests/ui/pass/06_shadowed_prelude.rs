#![deny(warnings)]

use zero_schema_macros::zero;

pub struct Result<T, E>(pub T, pub E);
#[allow(non_camel_case_types)]
pub struct u8;

pub struct Option<T>(pub T);
#[allow(non_snake_case)]
pub fn Some<T>(value: T) -> Option<T> {
    Option(value)
}
#[allow(non_upper_case_globals)]
pub const None: Option<()> = Option(());

#[zero(crate = zs)]
pub struct ShadowSafe {
    pub value: ::core::primitive::u8,
}

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ShadowKind {
    Value = 1,
}

#[zero(crate = zs)]
pub struct ShadowOptional {
    pub value: ::core::option::Option<ShadowKind>,
}

fn main() {
    let _ = Some(());
    let _ = None;
    let bytes = [1_u8];
    let _ = ShadowSafe::access(&bytes);
    let _ = ShadowKind::access(&bytes);
    let option_bytes = [0_u8];
    assert!(ShadowOptional::access(&option_bytes)
        .expect("all-zero optional field")
        .value()
        .is_none());
}
