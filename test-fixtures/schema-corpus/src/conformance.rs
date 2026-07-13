use core::ffi::CStr;
use widestring::{U16CStr, U16Str};
use zero_schema::ZeroSchema;

/// Canonical fixed-layout root mirrored by the unpublished C++ harness.
#[derive(Debug, PartialEq, ZeroSchema)]
pub struct ConformanceScalars {
    pub marker: u8,
    #[zero(endian = "little")]
    pub little16: u16,
    #[zero(endian = "big")]
    pub big32: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct ConformanceAligned {
    pub prefix: u8,
    #[zero(align = 8)]
    pub value: u32,
    pub suffix: u8,
}

#[derive(Debug, PartialEq, Eq, ZeroSchema)]
#[repr(u8)]
pub enum ConformanceTag {
    Empty = 10,
    Data = 11,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct ConformanceData {
    pub bits: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = ConformanceTag, tail = "zero")]
pub enum ConformanceMessage {
    #[zero(tag = ConformanceTag::Empty)]
    Empty,
    #[zero(tag = ConformanceTag::Data)]
    Data(ConformanceData),
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformancePrimitives {
    pub u8_value: u8,
    pub i8_bits: i8,
    pub bool_value: bool,
    #[zero(endian = "native")]
    pub u16_native: u16,
    #[zero(endian = "little")]
    pub u16_little: u16,
    #[zero(endian = "big")]
    pub u16_big: u16,
    #[zero(endian = "native")]
    pub i16_native: i16,
    #[zero(endian = "little")]
    pub i16_little: i16,
    #[zero(endian = "big")]
    pub i16_big: i16,
    #[zero(endian = "native")]
    pub u32_native: u32,
    #[zero(endian = "little")]
    pub u32_little: u32,
    #[zero(endian = "big")]
    pub u32_big: u32,
    #[zero(endian = "native")]
    pub i32_native: i32,
    #[zero(endian = "little")]
    pub i32_little: i32,
    #[zero(endian = "big")]
    pub i32_big: i32,
    #[zero(endian = "native")]
    pub u64_native: u64,
    #[zero(endian = "little")]
    pub u64_little: u64,
    #[zero(endian = "big")]
    pub u64_big: u64,
    #[zero(endian = "native")]
    pub i64_native: i64,
    #[zero(endian = "little")]
    pub i64_little: i64,
    #[zero(endian = "big")]
    pub i64_big: i64,
    #[zero(endian = "native")]
    pub f32_native: f32,
    #[zero(endian = "little")]
    pub f32_little: f32,
    #[zero(endian = "big")]
    pub f32_big: f32,
    #[zero(endian = "native")]
    pub f64_native: f64,
    #[zero(endian = "little")]
    pub f64_little: f64,
    #[zero(endian = "big")]
    pub f64_big: f64,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum ConformanceEnum8 {
    r#type = 0xa5,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "native")]
pub enum ConformanceEnumNative16 {
    Value = 0x1122,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "little")]
pub enum ConformanceEnumLittle16 {
    Value = 0x0102,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "big")]
pub enum ConformanceEnumBig16 {
    Value = 0x0102,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u32)]
#[zero(endian = "native")]
pub enum ConformanceEnumNative32 {
    Value = 0x1122_3344,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u32)]
#[zero(endian = "little")]
pub enum ConformanceEnumLittle32 {
    Value = 0x0102_0304,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u32)]
#[zero(endian = "big")]
pub enum ConformanceEnumBig32 {
    Value = 0x0102_0304,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformanceEnums {
    pub enum8: ConformanceEnum8,
    pub native16: ConformanceEnumNative16,
    pub little16: ConformanceEnumLittle16,
    pub big16: ConformanceEnumBig16,
    pub native32: ConformanceEnumNative32,
    pub little32: ConformanceEnumLittle32,
    pub big32: ConformanceEnumBig32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformanceStrings<'a> {
    #[zero(capacity = 3, len_type = u8, tail = "zero")]
    pub utf8_u8: &'a str,
    #[zero(capacity = 3, len_type = u16, endian = "native", tail = "zero")]
    pub utf8_u16_native: &'a str,
    #[zero(capacity = 3, len_type = u16, endian = "little", tail = "zero")]
    pub utf8_u16_little: &'a str,
    #[zero(capacity = 3, len_type = u16, endian = "big", tail = "zero")]
    pub utf8_u16_big: &'a str,
    #[zero(capacity = 1, len_type = u32, endian = "native", tail = "zero")]
    pub utf8_u32_native: &'a str,
    #[zero(capacity = 1, len_type = u32, endian = "little", tail = "zero")]
    pub utf8_u32_little: &'a str,
    #[zero(capacity = 1, len_type = u32, endian = "big", tail = "zero")]
    pub utf8_u32_big: &'a str,
    #[zero(capacity = 4, tail = "zero")]
    pub c_bytes: &'a CStr,
    #[zero(capacity = 1, len_type = u8, endian = "native", tail = "zero")]
    pub u16_u8: &'a U16Str,
    #[zero(capacity = 3, len_type = u16, endian = "native", tail = "zero")]
    pub u16_u16: &'a U16Str,
    #[zero(capacity = 1, len_type = u32, endian = "native", tail = "zero")]
    pub u16_u32: &'a U16Str,
    #[zero(capacity = 3, endian = "native", tail = "zero")]
    pub u16_c: &'a U16CStr,
    pub fixed: &'a [u8; 5],
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformanceNested {
    pub prefix: u8,
    pub child: ConformanceScalars,
    #[zero(endian = "big")]
    pub suffix: u16,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformanceExternalMessage {
    pub prefix: u8,
    pub tag: ConformanceTag,
    #[zero(tag_field = tag)]
    pub payload: ConformanceMessage,
    #[zero(endian = "big")]
    pub suffix: u16,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
pub enum ConformanceUnitTag {
    A = 21,
    B = 22,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = ConformanceUnitTag, tail = "zero")]
pub enum ConformanceUnits {
    #[zero(tag = ConformanceUnitTag::A)]
    A,
    #[zero(tag = ConformanceUnitTag::B)]
    B,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ConformanceExternalUnits {
    pub prefix: u8,
    pub tag: ConformanceUnitTag,
    #[zero(tag_field = tag, align = 8)]
    pub payload: ConformanceUnits,
    #[zero(endian = "little")]
    pub suffix: u16,
}

pub const CONFORMANCE_ROOT_IDS: &[(&str, u32)] = &[
    ("conformance-scalars", 1001),
    ("conformance-aligned", 1002),
    ("conformance-message-empty", 1003),
    ("conformance-message-data", 1004),
    ("conformance-primitives", 1005),
    ("conformance-enums", 1006),
    ("conformance-strings", 1007),
    ("conformance-nested", 1008),
    ("conformance-zst-layout", 1009),
    ("conformance-external-message", 1010),
    ("conformance-external-units", 1011),
];
