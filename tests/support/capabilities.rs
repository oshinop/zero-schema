use core::ffi::CStr;

use widestring::{U16CStr, U16Str};
use zero_schema::zero;

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

#[zero]
#[derive(Debug, PartialEq)]
pub struct FixedBytes<'a, const N: usize> {
    pub bytes: &'a [u8; N],
}
