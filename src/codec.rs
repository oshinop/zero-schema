use core::{ffi::CStr, str::Utf8Error};

use widestring::{U16CStr, U16Str};
use zerocopy::IntoBytes;

use crate::{encode::Prezeroed, error::LayoutError, wire::LengthWire};

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    LengthOutOfBounds { length: usize, capacity: usize },
    InvalidUtf8(Utf8Error),
    MissingNul,
    NonZeroTail { offset: usize },
    CapacityExceeded { length: usize, capacity: usize },
}

#[inline]
fn logical_len<L: LengthWire>(length: &L, capacity: usize) -> Result<usize, CodecError> {
    let length = length.to_usize().ok_or(CodecError::LengthOutOfBounds {
        length: usize::MAX,
        capacity,
    })?;
    if length > capacity {
        return Err(CodecError::LengthOutOfBounds { length, capacity });
    }
    Ok(length)
}

#[inline]
fn check_byte_tail(bytes: &[u8], start: usize) -> Result<(), CodecError> {
    if let Some(relative) = bytes
        .get(start..)
        .unwrap_or(&[])
        .iter()
        .position(|&byte| byte != 0)
    {
        return Err(CodecError::NonZeroTail {
            offset: start + relative,
        });
    }
    Ok(())
}

#[inline]
fn check_unit_tail(units: &[u16], start: usize) -> Result<(), CodecError> {
    if let Some(relative) = units
        .get(start..)
        .unwrap_or(&[])
        .iter()
        .position(|&unit| unit != 0)
    {
        return Err(CodecError::NonZeroTail {
            offset: start + relative,
        });
    }
    Ok(())
}

#[doc(hidden)]
pub fn decode_str<'a, L: LengthWire>(
    length: &L,
    data: &'a [u8],
    zero_tail: bool,
) -> Result<&'a str, CodecError> {
    let length = logical_len(length, data.len())?;
    let value = core::str::from_utf8(&data[..length]).map_err(CodecError::InvalidUtf8)?;
    if zero_tail {
        check_byte_tail(data, length)?;
    }
    Ok(value)
}

#[doc(hidden)]
pub fn decode_c_str(data: &[u8], zero_tail: bool) -> Result<&CStr, CodecError> {
    let value = CStr::from_bytes_until_nul(data).map_err(|_| CodecError::MissingNul)?;
    if zero_tail {
        check_byte_tail(data, value.to_bytes_with_nul().len())?;
    }
    Ok(value)
}

#[doc(hidden)]
pub fn decode_u16_str<'a, L: LengthWire>(
    length: &L,
    units: &'a [u16],
    zero_tail: bool,
) -> Result<&'a U16Str, CodecError> {
    let length = logical_len(length, units.len())?;
    if zero_tail {
        check_unit_tail(units, length)?;
    }
    Ok(U16Str::from_slice(&units[..length]))
}

#[doc(hidden)]
pub fn decode_u16_c_str(units: &[u16], zero_tail: bool) -> Result<&U16CStr, CodecError> {
    let value = U16CStr::from_slice_truncate(units).map_err(|_| CodecError::MissingNul)?;
    if zero_tail {
        check_unit_tail(units, value.as_slice_with_nul().len())?;
    }
    Ok(value)
}

#[doc(hidden)]
pub fn validate_str_encode(value: &str, capacity: usize) -> Result<(), CodecError> {
    capacity_check(value.len(), capacity)
}
#[doc(hidden)]
pub fn validate_c_str_encode(value: &CStr, capacity: usize) -> Result<(), CodecError> {
    capacity_check(value.to_bytes_with_nul().len(), capacity)
}
#[doc(hidden)]
pub fn validate_u16_str_encode(value: &U16Str, capacity: usize) -> Result<(), CodecError> {
    capacity_check(value.len(), capacity)
}
#[doc(hidden)]
pub fn validate_u16_c_str_encode(value: &U16CStr, capacity: usize) -> Result<(), CodecError> {
    capacity_check(value.as_slice_with_nul().len(), capacity)
}

#[inline]
fn capacity_check(length: usize, capacity: usize) -> Result<(), CodecError> {
    if length > capacity {
        Err(CodecError::CapacityExceeded { length, capacity })
    } else {
        Ok(())
    }
}

#[doc(hidden)]
pub fn encode_length<L: LengthWire>(
    length: usize,
    destination: &mut Prezeroed<'_>,
) -> Result<(), LayoutError> {
    let encoded = L::encoded(length).ok_or(LayoutError::OffsetOverflow)?;
    encoded.write_to(destination)
}

#[doc(hidden)]
pub fn encode_str(value: &str, destination: &mut Prezeroed<'_>) -> Result<(), LayoutError> {
    destination.write(0, value.as_bytes())
}
#[doc(hidden)]
pub fn encode_c_str(value: &CStr, destination: &mut Prezeroed<'_>) -> Result<(), LayoutError> {
    destination.write(0, value.to_bytes_with_nul())
}
#[doc(hidden)]
pub fn encode_u16_str(value: &U16Str, destination: &mut Prezeroed<'_>) -> Result<(), LayoutError> {
    destination.write(0, value.as_slice().as_bytes())
}
#[doc(hidden)]
pub fn encode_u16_c_str(
    value: &U16CStr,
    destination: &mut Prezeroed<'_>,
) -> Result<(), LayoutError> {
    destination.write(0, value.as_slice_with_nul().as_bytes())
}
#[doc(hidden)]
pub fn encode_fixed_bytes<const N: usize>(
    value: &[u8; N],
    destination: &mut Prezeroed<'_>,
) -> Result<(), LayoutError> {
    destination.write(0, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode::Prezeroed, wire::U8};

    #[test]
    fn length_prefixed_boundaries_and_tails() {
        assert_eq!(decode_str(&U8::new(0), b"", true), Ok(""));
        assert_eq!(decode_str(&U8::new(3), b"abc", true), Ok("abc"));
        assert_eq!(
            decode_str(&U8::new(4), b"abc", false),
            Err(CodecError::LengthOutOfBounds {
                length: 4,
                capacity: 3
            })
        );
        match decode_str(&U8::new(1), &[0xff], false).unwrap_err() {
            CodecError::InvalidUtf8(source) => {
                assert_eq!(source.valid_up_to(), 0);
                assert_eq!(source.error_len(), Some(1));
            }
            other => panic!("expected invalid UTF-8, got {other:?}"),
        }
        assert_eq!(
            decode_str(&U8::new(1), b"ax", true),
            Err(CodecError::NonZeroTail { offset: 1 })
        );
    }

    #[test]
    fn c_strings_use_first_nul() {
        assert_eq!(decode_c_str(b"a\0b", false).unwrap().to_bytes(), b"a");
        assert_eq!(decode_c_str(b"a\0\0", true).unwrap().to_bytes(), b"a");
        assert_eq!(
            decode_c_str(b"a\0b", true),
            Err(CodecError::NonZeroTail { offset: 2 })
        );
        assert_eq!(decode_c_str(b"abc", false), Err(CodecError::MissingNul));
        assert_eq!(decode_c_str(b"\0", true).unwrap().to_bytes(), b"");
    }

    #[test]
    fn wide_strings_count_code_units() {
        let unpaired = [0xd800, 0];
        assert_eq!(
            decode_u16_str(&U8::new(1), &unpaired, true)
                .unwrap()
                .as_slice(),
            &[0xd800]
        );
        assert_eq!(
            decode_u16_str(&U8::new(3), &unpaired, false),
            Err(CodecError::LengthOutOfBounds {
                length: 3,
                capacity: 2
            })
        );
        assert_eq!(
            decode_u16_c_str(&[0xd800, 0, 7], false).unwrap().as_slice(),
            &[0xd800]
        );
        assert_eq!(
            decode_u16_c_str(&[0xd800, 0, 7], true),
            Err(CodecError::NonZeroTail { offset: 2 })
        );
        assert_eq!(
            decode_u16_c_str(&[1, 2], false),
            Err(CodecError::MissingNul)
        );
    }

    #[test]
    fn capacity_and_confined_writes() {
        assert_eq!(
            validate_str_encode("abc", 2),
            Err(CodecError::CapacityExceeded {
                length: 3,
                capacity: 2
            })
        );
        let c = c"ab";
        assert_eq!(
            validate_c_str_encode(c, 2),
            Err(CodecError::CapacityExceeded {
                length: 3,
                capacity: 2
            })
        );
        let mut bytes = [0xa5; 8];
        {
            let mut root = Prezeroed::new(&mut bytes[1..7]);
            let mut field = root.subrange(2, 3).unwrap();
            encode_str("abc", &mut field).unwrap();
        }
        assert_eq!(bytes, [0xa5, 0, 0, b'a', b'b', b'c', 0, 0xa5]);
    }
}
