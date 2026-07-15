use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tag {
    One = 1,
}

#[zero]
struct Child {
    required: Tag,
}

#[zero]
enum TaggedPayload {
    #[zero(tag = Tag::One)]
    One(Child),
}

#[zero]
struct Invalid {
    value: Option<[TaggedPayload; 2]>,
}

fn main() {}
