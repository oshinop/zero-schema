#![no_std]

use core::ffi::CStr;
use widestring::{U16CStr, U16Str};
use zero_schema::ZeroSchema;

#[derive(ZeroSchema)]
#[repr(u8)]
enum SmokeTag {
    Empty = 1,
    Data = 2,
}

#[derive(ZeroSchema)]
struct Borrowed<'a> {
    #[zero(capacity = 4, len_type = u8)]
    text: &'a str,
    #[zero(capacity = 4)]
    c_text: &'a CStr,
    #[zero(capacity = 3, len_type = u8)]
    wide: &'a U16Str,
    #[zero(capacity = 3)]
    wide_c: &'a U16CStr,
    fixed: &'a [u8; 3],
}

#[derive(ZeroSchema)]
struct Number {
    value: u32,
}

#[derive(ZeroSchema)]
#[zero(tag = SmokeTag)]
enum Packet {
    #[zero(tag = SmokeTag::Empty)]
    Empty,
    #[zero(tag = SmokeTag::Data)]
    Data(Number),
}

pub fn smoke_roundtrip() -> u32 {
    let units = [0x41, 0x42];
    let nul_units = [0x43, 0];
    let value = Borrowed {
        text: "rust",
        c_text: c"zs",
        wide: U16Str::from_slice(&units),
        wide_c: match U16CStr::from_slice(&nul_units) {
            Ok(value) => value,
            Err(_) => return 1,
        },
        fixed: b"raw",
    };
    let buffer = match value.encode() {
        Ok(buffer) => buffer,
        Err(_) => return 2,
    };
    let parsed = match Borrowed::parse(buffer.as_bytes()) {
        Ok(value) => value,
        Err(_) => return 3,
    };
    if parsed.text != "rust"
        || parsed.c_text.to_bytes() != b"zs"
        || parsed.wide.as_slice() != units
        || parsed.wide_c.as_slice() != [0x43]
        || parsed.fixed != b"raw"
    {
        return 4;
    }
    let packet = match Packet::Data(Number { value: 7 }).encode() {
        Ok(buffer) => buffer,
        Err(_) => return 5,
    };
    match Packet::parse(packet.as_bytes()) {
        Ok(Packet::Data(number)) if number.value == 7 => 0,
        _ => 6,
    }
}

pub fn smoke_prefix() -> u32 {
    let mut input = [0u8; Packet::WIRE_SIZE + 3];
    let encoded = match Packet::Empty.encode() {
        Ok(buffer) => buffer,
        Err(_) => return 10,
    };
    input[..Packet::WIRE_SIZE].copy_from_slice(encoded.as_bytes());
    input[Packet::WIRE_SIZE..].copy_from_slice(&[7, 8, 9]);
    match Packet::parse_prefix(&input) {
        Ok((Packet::Empty, rest)) if rest == [7, 8, 9] => 0,
        _ => 11,
    }
}
