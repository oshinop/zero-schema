#![no_std]

use core::ffi::CStr;
pub mod conformance;
use widestring::{U16CStr, U16Str};
use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, Eq, ZeroSchema)]
#[repr(u8)]
pub enum CorpusCode8 {
    Zero = 0,
    Max = 255,
}

#[derive(Debug, PartialEq, Eq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "big")]
pub enum CorpusCode16Be {
    Marker = 0x1234,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct EndianMatrix {
    pub byte: u8,
    #[zero(endian = "little")]
    pub little: u16,
    #[zero(endian = "big")]
    pub big: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct StringMatrix<'a> {
    #[zero(capacity = 3, len_type = u8, tail = "zero")]
    pub text: &'a str,
    #[zero(capacity = 4, tail = "zero")]
    pub c_text: &'a CStr,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct FuzzAllStrings<'a> {
    #[zero(capacity = 3, len_type = u8, tail = "zero")]
    pub text: &'a str,
    #[zero(capacity = 4, tail = "zero")]
    pub c_text: &'a CStr,
    #[zero(capacity = 3, len_type = u8, tail = "zero")]
    pub wide: &'a U16Str,
    #[zero(capacity = 3, tail = "zero")]
    pub wide_c: &'a U16CStr,
}

#[derive(Debug, PartialEq, ZeroSchema)]
pub struct CorpusPayload {
    pub value: u32,
}

#[derive(Debug, PartialEq, Eq, ZeroSchema)]
#[repr(u8)]
pub enum CorpusTag {
    Unit = 1,
    Payload = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = CorpusTag, tail = "zero")]
pub enum CorpusMessage {
    #[zero(tag = CorpusTag::Unit)]
    Unit,
    #[zero(tag = CorpusTag::Payload)]
    Payload(CorpusPayload),
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
pub struct ExternalCorpusMessage {
    pub tag: CorpusTag,
    #[zero(tag_field = tag)]
    pub payload: CorpusMessage,
}

pub const ROOT_IDS: &[&str] = &["1", "2", "3", "4", "5", "6"];

pub const FUZZ_TARGETS: &[(&str, &str, u8)] = &[
    ("1", "parse_message", 1),
    ("2", "parse_message", 2),
    ("3", "parse_message", 3),
    ("4", "parse_external_tag", 1),
    ("5", "parse_all_strings", 1),
    ("6", "roundtrip_message", 1),
];
