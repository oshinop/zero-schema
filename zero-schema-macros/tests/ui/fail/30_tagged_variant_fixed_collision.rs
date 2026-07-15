use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tag {
    Value = 1,
}

#[zero(crate = zs)]
struct Child {
    value: u8,
}

#[zero(crate = zs)]
enum Payload {
    #[zero(tag = Tag::Value)]
    tag(Child),
}

fn main() {}
