use zero_schema::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
    Two = 2,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Child {
    pub required: Required,
    pub payload: u32,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleTextChild<'a> {
    pub required: Required,
    #[zero(capacity = 4, len_type = u8)]
    pub text: &'a str,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaggedPayload {
    pub required: Required,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tagged {
    #[zero(tag = Required::One)]
    One(TaggedPayload),
    #[zero(tag = Required::Two)]
    Two(TaggedPayload),
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleTaggedRecord {
    pub required: Required,
    pub tag: Required,
    #[zero(tag_field = tag)]
    pub payload: Tagged,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalRoot<'a> {
    pub before: u8,
    #[zero(align = 8)]
    pub maybe_kind: Option<Required>,
    pub maybe_child: Option<Child>,
    pub maybe_array: Option<[Required; 2]>,
    pub maybe_text: Option<EligibleTextChild<'a>>,
    pub maybe_tagged: Option<EligibleTaggedRecord>,
    pub after: u8,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaddedEligibleChild {
    pub required: Required,
    pub payload: u32,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaddedOptionalArrayRoot {
    pub before: u8,
    pub children: Option<[PaddedEligibleChild; 2]>,
    pub after: u8,
}

pub fn padded_array_field() -> &'static zero_schema::FieldDescriptor {
    PaddedOptionalArrayRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "children")
        .expect("declared padded optional array metadata")
}

pub fn padded_array_bytes() -> [u8; PaddedOptionalArrayRoot::SCHEMA_SIZE] {
    let mut bytes = [0_u8; PaddedOptionalArrayRoot::SCHEMA_SIZE];
    bytes[PaddedOptionalArrayRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "before")
        .expect("declared prefix field")
        .offset()] = 0x31;
    bytes[PaddedOptionalArrayRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "after")
        .expect("declared suffix field")
        .offset()] = 0x53;
    bytes
}

pub fn field(name: &str) -> &'static zero_schema::FieldDescriptor {
    OptionalRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("declared optional field metadata")
}

pub fn optional_root_bytes() -> [u8; OptionalRoot::SCHEMA_SIZE] {
    let mut bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    bytes[field("before").offset()] = 0x31;
    bytes[field("after").offset()] = 0x53;
    bytes
}
