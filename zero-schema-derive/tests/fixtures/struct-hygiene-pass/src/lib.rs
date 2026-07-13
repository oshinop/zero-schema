#![deny(warnings)]
use zero_schema_derive::ZeroSchema;

#[derive(ZeroSchema)]
#[zero(crate = zs)]
pub struct Names<'a, #[allow(dead_code)] const W0: usize> {
    __end: u8,
    bytes: &'a [u8; W0],
}

#[derive(ZeroSchema)]
#[zero(crate = zs)]
struct Child { value: u8 }

#[derive(ZeroSchema)]
#[zero(crate = zs)]
pub struct Combined<'a, const W0: usize> { child: Child, bytes: &'a [u8; W0] }

pub const ROOT_BYTES: usize = 1;

pub mod rebased {
    use super::*;

    pub const LOCAL_BYTES: usize = 1;

    #[derive(ZeroSchema)]
    #[zero(crate = zs)]
    pub struct Paths<'a> {
        local: &'a [u8; self::LOCAL_BYTES],
        root: &'a [u8; super::ROOT_BYTES],
    }

}

pub mod shadows {
    use super::*;

    #[allow(non_camel_case_types)]
    pub type u8 = ::core::primitive::u8;
    #[allow(non_camel_case_types)]
    pub type u16 = ::core::primitive::u16;
    #[allow(non_camel_case_types)]
    pub type usize = ::core::primitive::usize;
    #[allow(non_camel_case_types)]
    pub type str = ::core::primitive::str;

    #[derive(ZeroSchema)]
    #[zero(crate = zs)]
    pub struct Shadowed<'a> {
        number: u8,
        #[zero(capacity = 1, len_type = u16)]
        text: &'a str,
    }
}

pub mod restricted {
    use super::*;

    #[derive(ZeroSchema)]
    #[zero(crate = zs)]
    pub(super) struct ParentVisible {
        value: u8,
    }

    pub fn value() -> u8 {
        ParentVisible { value: 7 }.value
    }
}
