use zero_schema::zero;

#[allow(non_camel_case_types)]
#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
#[zero(endian = "big")]
pub enum BigCode {
    Ready = 0x0102,
    r#type = 0xabcd,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
#[zero(endian = "little")]
pub enum LittleCode {
    First = 0x0102_0304,
    Last = 0xffff_fffe,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum NativeCode {
    Marker = 0x1122_3344,
    Maximum = 0xffff_ffff,
}

#[zero]
pub struct DirectChild {
    pub valid: bool,
    pub value: u16,
}

#[zero]
pub struct GenericBytes<'a, const N: usize> {
    pub bytes: &'a [u8; N],
}

#[zero(borrow = 'a)]
pub struct BorrowedChild<'a> {
    #[zero(capacity = 8)]
    pub text: &'a str,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildTag {
    Empty = 1,
    Data = 2,
    Spare = 3,
}

#[zero]
pub struct TaggedData {
    pub number: u32,
}

#[zero]
pub enum ChildMessage {
    #[zero(tag = ChildTag::Empty)]
    Empty,
    #[zero(tag = ChildTag::Data)]
    Data(TaggedData),
}

// Keeping a primitive after the nested projection exercises the downstream
// associated-wire projection when a child is not the trailing field.
#[zero]
pub struct TrailingProjection {
    pub child: DirectChild,
    pub sentinel: u8,
}

/// A closed scalar whose all-zero representation is invalid, so downstream
/// schemas can use it as a zero-sentinel optional payload.
#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionalCode {
    One = 1,
    Two = 2,
}

/// A nonzero record path for downstream optional and fixed-array composition.
#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalChild {
    pub code: OptionalCode,
    pub payload: u16,
}
