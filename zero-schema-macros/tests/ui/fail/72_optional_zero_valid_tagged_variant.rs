use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    Zero = 0,
    Unit = 1,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
}

#[zero]
pub struct Child {
    required: Required,
}

#[zero]
pub enum Payload {
    #[zero(tag = Tag::Zero)]
    Data(Child),
    #[zero(tag = Tag::Unit)]
    Unit,
}

#[zero]
pub struct Record {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Payload,
}

#[zero]
pub struct Invalid {
    value: Option<Record>,
}

fn main() {}
