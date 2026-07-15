#![deny(warnings)]
#![allow(non_camel_case_types)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    Type = 1,
}

#[zero(crate = zs)]
pub struct Child {
    value: u8,
}

#[zero(crate = zs)]
pub enum Payload {
    #[zero(tag = Tag::Type)]
    r#type(Child),
}

#[zero(crate = zs)]
pub struct Root {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Payload,
}

fn main() {
    let bytes = [Tag::Type as u8, 7];
    let root = Root::access(&bytes).unwrap();
    assert_eq!(root.payload().r#type().unwrap().value(), 7);
}
