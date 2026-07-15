use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tag {
    Some = 1,
}

#[zero]
struct Invalid {
    tag: Tag,
    #[zero(tag_field = tag)]
    value: Option<Missing>,
}

fn main() {}
