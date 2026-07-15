#![deny(warnings)]

use zs::zero;

#[zero(crate = zs, endian = "little", borrow = 'a)]
#[derive(Clone, Debug)]
pub struct Record<'a, const N: usize> {
    pub sequence: u32,
    #[zero(capacity = 8, len_type = u16)]
    pub name: &'a str,
    pub bytes: &'a [u8; N],
    pub samples: [u16; N],
}

#[zero(crate = zs, endian = "big")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Kind {
    Alpha = 1,
    Beta = 2,
}
#[zero(crate = zs)]
pub struct Child {
    pub value: u8,
}

#[zero(crate = zs)]
pub enum Payload {
    #[zero(tag = Kind::Alpha)]
    Empty,
    #[zero(tag = Kind::Beta)]
    Data(Child),
}

#[zero(crate = zs)]
pub struct Message {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: Payload,
}


fn main() {
    let _ = Record::<'static, 3>::SCHEMA_SIZE;
    let _ = Record::<'static, 3>::SCHEMA_ALIGN;
    let _ = Record::<'static, 3>::SCHEMA_STRIDE;
    let _ = Record::<'static, 3>::LAYOUT;
    let _ = Kind::SCHEMA_SIZE;
    let _ = zs::schema_buffer!(Record<'static, 3>);
    let _ = Message::SCHEMA_SIZE;
}
