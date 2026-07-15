#![deny(warnings)]

use core::ffi::CStr;
use zero_schema_macros::zero;

mod payload_schema {
    use super::*;

    #[zero(crate = zs)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u8)]
    pub enum Tag {
        Unit = 1,
        Data = 2,
    }

    #[zero(crate = zs)]
    pub struct Child<'a> {
        pub value: u32,
        #[zero(capacity = 4)]
        pub name: &'a CStr,
    }

    #[zero(crate = zs)]
    pub enum Payload<'a> {
        #[zero(tag = Tag::Unit)]
        Unit,
        #[zero(tag = Tag::Data)]
        Data(Child<'a>),
    }
}

#[zero(crate = zs, align = 8)]
pub struct Root<'a, const N: usize> {
    pub tag: payload_schema::Tag,
    #[zero(tag_field = tag)]
    pub payload: payload_schema::Payload<'a>,
    pub values: [u32; N],
}

#[zero(crate = zs)]
pub struct TagAfterPayload<'a> {
    #[zero(tag_field = tag)]
    pub payload: payload_schema::Payload<'a>,
    pub tag: payload_schema::Tag,
}

fn main() {
    let _ = Root::<'static, 2>::SCHEMA_SIZE;
    let _ = Root::<'static, 2>::SCHEMA_ALIGN;
    let _ = Root::<'static, 2>::SCHEMA_STRIDE;
}
