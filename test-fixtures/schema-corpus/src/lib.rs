#![no_std]

use core::ffi::CStr;
pub mod conformance;
use widestring::{U16CStr, U16Str};
use zero_schema::zero;

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CorpusCode8 {
    Zero = 0,
    Max = 255,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
#[zero(endian = "big")]
pub enum CorpusCode16Be {
    Marker = 0x1234,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct EndianMatrix {
    pub byte: u8,
    #[zero(endian = "little")]
    pub little: u16,
    #[zero(endian = "big")]
    pub big: u32,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct StringMatrix<'a> {
    #[zero(capacity = 3, len_type = u8)]
    pub text: &'a str,
    #[zero(capacity = 4)]
    pub c_text: &'a CStr,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct FuzzAllStrings<'a> {
    #[zero(capacity = 3, len_type = u8)]
    pub text: &'a str,
    #[zero(capacity = 4)]
    pub c_text: &'a CStr,
    #[zero(capacity = 3, len_type = u8)]
    pub wide: &'a U16Str,
    #[zero(capacity = 3)]
    pub wide_c: &'a U16CStr,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct CorpusPayload {
    pub value: u32,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CorpusTag {
    Unit = 1,
    Payload = 2,
    Reserved = 3,
}

#[zero]
#[derive(Debug, PartialEq)]
pub enum CorpusMessage {
    #[zero(tag = CorpusTag::Unit)]
    Unit,
    #[zero(tag = CorpusTag::Payload)]
    Payload(CorpusPayload),
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct ExternalCorpusMessage {
    pub tag: CorpusTag,
    #[zero(tag_field = tag)]
    pub payload: CorpusMessage,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConfigKind {
    File = 1,
    Memory = 2,
    Reserved = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header<'a> {
    pub version: u16,
    #[zero(capacity = 6)]
    pub producer: &'a CStr,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryConfig {
    pub capacity: u16,
    pub enabled: bool,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileConfig<'a> {
    pub header: Header<'a>,
    pub flags: u32,
}

#[zero]
#[derive(Debug, PartialEq)]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
}

#[zero(align = 16)]
#[derive(Debug, PartialEq)]
pub struct AllFeatures<'a> {
    pub sequence: u64,
    pub active: bool,
    pub priority: Priority,
    #[zero(capacity = 7, len_type = u8)]
    pub name: &'a str,
    #[zero(capacity = 6)]
    pub c_name: &'a CStr,
    #[zero(capacity = 2, len_type = u8, align = 4)]
    pub wide: &'a U16Str,
    #[zero(capacity = 3)]
    pub wide_c: &'a U16CStr,
    pub token: &'a [u8; 5],
    pub header: Header<'a>,
    pub samples: [u32; 3],
    pub headers: [Header<'a>; 2],
    pub config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    pub config: Config<'a>,
    pub checksum: u8,
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
