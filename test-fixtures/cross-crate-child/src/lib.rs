use zero_schema::ZeroSchema;

#[allow(non_camel_case_types)]
#[derive(ZeroSchema)]
#[repr(u16)]
#[zero(endian = "big")]
pub enum BigCode {
    Ready = 0x0102,
    r#type = 0xabcd,
}

#[derive(ZeroSchema, Debug, Eq, PartialEq)]
#[repr(u32)]
#[zero(endian = "little")]
pub enum LittleCode {
    First = 0x0102_0304,
    Last = 0xffff_fffe,
}

#[derive(ZeroSchema)]
#[repr(u32)]
pub enum NativeCode {
    Marker = 0x1122_3344,
    Maximum = 0xffff_ffff,
}

#[derive(ZeroSchema)]
pub struct DirectChild {
    pub valid: bool,
    pub value: u16,
}

#[derive(ZeroSchema)]
pub struct GenericBytes<'a, const N: usize> {
    pub bytes: &'a [u8; N],
}

#[derive(ZeroSchema)]
#[zero(borrow = 'a)]
pub struct BorrowedChild<'a> {
    #[zero(capacity = 8)]
    pub text: &'a str,
}

#[derive(ZeroSchema)]
#[repr(u8)]
pub enum ChildTag {
    Empty = 1,
    Data = 2,
}

#[derive(ZeroSchema)]
pub struct TaggedData {
    pub number: u32,
}

#[derive(ZeroSchema)]
#[zero(tag = ChildTag, tail = "zero")]
pub enum ChildMessage {
    #[zero(tag = ChildTag::Empty)]
    Empty,
    #[zero(tag = ChildTag::Data)]
    Data(TaggedData),
}

// Keeping a primitive after the nested projection exercises KnownLayout's trailing-field rule.
#[derive(ZeroSchema)]
pub struct TrailingProjection {
    pub child: DirectChild,
    pub sentinel: u8,
}
