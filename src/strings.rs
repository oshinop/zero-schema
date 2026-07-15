//! Bounded string proof and mutation helpers.
//!
//! Proof reads only the active length or the first terminator. Mutation checks
//! the complete source representation before it touches a selected destination
//! slice and deliberately leaves capacity after the new active value intact.

use core::{ffi::CStr, str::Utf8Error};
use zerocopy::FromBytes;

use widestring::{U16CStr, U16Str};

use crate::{error::LayoutError, wire::LengthWire};

/// A bounded-string representation failure that generated access errors map to
/// their root-specific, structured variants.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringProofError {
    LengthOutOfBounds { length: usize, capacity: usize },
    InvalidUtf8(Utf8Error),
    MissingNul,
}

/// A bounded-string mutation preflight failure that generated mutation errors
/// map to their root-specific, structured variants.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringMutationError {
    CapacityExceeded { length: usize, capacity: usize },
    LengthUnrepresentable { length: usize },
    PrefixSize { expected: usize, actual: usize },
    WideByteSize { actual: usize },
    Layout(LayoutError),
}

impl From<LayoutError> for StringMutationError {
    #[inline]
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

#[inline]
fn active_length<L: LengthWire>(length: &L, capacity: usize) -> Result<usize, StringProofError> {
    let length = length.load().ok_or(StringProofError::LengthOutOfBounds {
        length: usize::MAX,
        capacity,
    })?;
    if length > capacity {
        return Err(StringProofError::LengthOutOfBounds { length, capacity });
    }
    Ok(length)
}

/// Proves a length-prefixed UTF-8 value without reading unused capacity.
#[doc(hidden)]
#[inline]
pub fn prove_str<'data, L: LengthWire>(
    length: &L,
    data: &'data [u8],
) -> Result<&'data str, StringProofError> {
    let length = active_length(length, data.len())?;
    core::str::from_utf8(&data[..length]).map_err(StringProofError::InvalidUtf8)
}

/// Proves a nul-terminated narrow string without reading after its first nul.
#[doc(hidden)]
#[inline]
pub fn prove_c_str(data: &[u8]) -> Result<&CStr, StringProofError> {
    CStr::from_bytes_until_nul(data).map_err(|_| StringProofError::MissingNul)
}

/// Proves a length-prefixed native-endian `u16` value without checking Unicode
/// scalar validity or reading unused unit capacity.
#[doc(hidden)]
#[inline]
pub fn prove_u16_str<'units, L: LengthWire>(
    length: &L,
    units: &'units [u16],
) -> Result<&'units U16Str, StringProofError> {
    let length = active_length(length, units.len())?;
    Ok(U16Str::from_slice(&units[..length]))
}

/// Proves a nul-terminated native-endian `u16` value without checking Unicode
/// scalar validity or reading units after the first terminator.
#[doc(hidden)]
#[inline]
pub fn prove_u16_c_str(units: &[u16]) -> Result<&U16CStr, StringProofError> {
    U16CStr::from_slice_truncate(units).map_err(|_| StringProofError::MissingNul)
}

/// Proves a length-prefixed native-endian `u16` value from one exact, aligned
/// declared storage subrange without exposing its aggregate wire wrapper.
#[doc(hidden)]
#[inline]
pub fn prove_u16_str_bytes<'units, L: LengthWire, const N: usize>(
    length: &L,
    bytes: &'units [u8],
) -> Result<&'units U16Str, StringProofError> {
    let units = match <[u16; N]>::ref_from_bytes(bytes) {
        Ok(units) => units,
        Err(_) => unreachable!("selected u16 field storage remains exact and aligned"),
    };
    prove_u16_str(length, units)
}

/// Proves a nul-terminated native-endian `u16` value from one exact, aligned
/// declared storage subrange without exposing its aggregate wire wrapper.
#[doc(hidden)]
#[inline]
pub fn prove_u16_c_str_bytes<const N: usize>(bytes: &[u8]) -> Result<&U16CStr, StringProofError> {
    let units = match <[u16; N]>::ref_from_bytes(bytes) {
        Ok(units) => units,
        Err(_) => unreachable!("selected u16 field storage remains exact and aligned"),
    };
    prove_u16_c_str(units)
}

/// Preflights a length-prefixed write before either data or prefix changes.
#[doc(hidden)]
#[inline]
pub fn preflight_length_prefixed<L: LengthWire>(
    length: usize,
    capacity: usize,
    prefix_size: usize,
) -> Result<(), StringMutationError> {
    if prefix_size != L::WIDTH {
        return Err(StringMutationError::PrefixSize {
            expected: L::WIDTH,
            actual: prefix_size,
        });
    }
    if length > capacity {
        return Err(StringMutationError::CapacityExceeded { length, capacity });
    }
    if !L::represents(length) {
        return Err(StringMutationError::LengthUnrepresentable { length });
    }
    Ok(())
}

#[inline]
fn wide_capacity(bytes: &[u8]) -> Result<usize, StringMutationError> {
    if bytes.len() % size_of::<u16>() != 0 {
        return Err(StringMutationError::WideByteSize {
            actual: bytes.len(),
        });
    }
    Ok(bytes.len() / size_of::<u16>())
}

#[inline]
fn store_native_u16s(destination: &mut [u8], source: &[u16]) {
    for (destination, source) in destination.chunks_exact_mut(size_of::<u16>()).zip(source) {
        destination.copy_from_slice(&source.to_ne_bytes());
    }
}

/// Preflights a length-prefixed UTF-8 replacement without touching storage.
#[doc(hidden)]
#[inline]
pub fn preflight_str<L: LengthWire>(
    prefix: &[u8],
    data: &[u8],
    value: &str,
) -> Result<(), StringMutationError> {
    preflight_length_prefixed::<L>(value.len(), data.len(), prefix.len())
}

/// Commits a previously preflighted length-prefixed UTF-8 replacement.
///
/// Callers must have established [`preflight_str`] for exactly these selected
/// prefix/data ranges. The write leaves capacity after the active value intact.
#[doc(hidden)]
#[inline]
pub fn commit_str<L: LengthWire>(prefix: &mut [u8], data: &mut [u8], value: &str) {
    data[..value.len()].copy_from_slice(value.as_bytes());
    L::store_preflighted(value.len(), prefix);
}

/// Replaces a length-prefixed UTF-8 value without clearing unused destination
/// capacity.
#[doc(hidden)]
#[inline]
pub fn set_str<L: LengthWire>(
    prefix: &mut [u8],
    data: &mut [u8],
    value: &str,
) -> Result<(), StringMutationError> {
    preflight_str::<L>(prefix, data, value)?;
    commit_str::<L>(prefix, data, value);
    Ok(())
}

/// Preflights a length-prefixed native-endian `u16` replacement without
/// touching storage.
#[doc(hidden)]
#[inline]
pub fn preflight_u16_str<L: LengthWire>(
    prefix: &[u8],
    data: &[u8],
    value: &U16Str,
) -> Result<(), StringMutationError> {
    let capacity = wide_capacity(data)?;
    preflight_length_prefixed::<L>(value.len(), capacity, prefix.len())
}

/// Commits a previously preflighted native-endian `u16` replacement.
#[doc(hidden)]
#[inline]
pub fn commit_u16_str<L: LengthWire>(prefix: &mut [u8], data: &mut [u8], value: &U16Str) {
    let active = core::mem::size_of_val(value.as_slice());
    store_native_u16s(&mut data[..active], value.as_slice());
    L::store_preflighted(value.len(), prefix);
}

/// Replaces a length-prefixed native-endian `u16` value without clearing unused
/// unit capacity. `data` is wire bytes, never a mutable typed view.
#[doc(hidden)]
#[inline]
pub fn set_u16_str<L: LengthWire>(
    prefix: &mut [u8],
    data: &mut [u8],
    value: &U16Str,
) -> Result<(), StringMutationError> {
    preflight_u16_str::<L>(prefix, data, value)?;
    commit_u16_str::<L>(prefix, data, value);
    Ok(())
}

/// Preflights a bounded narrow C string replacement without touching storage.
#[doc(hidden)]
#[inline]
pub fn preflight_c_str(data: &[u8], value: &CStr) -> Result<(), StringMutationError> {
    let bytes = value.to_bytes_with_nul();
    if bytes.len() > data.len() {
        return Err(StringMutationError::CapacityExceeded {
            length: bytes.len(),
            capacity: data.len(),
        });
    }
    Ok(())
}

/// Commits a previously preflighted bounded narrow C string replacement.
#[doc(hidden)]
#[inline]
pub fn commit_c_str(data: &mut [u8], value: &CStr) {
    let bytes = value.to_bytes_with_nul();
    data[..bytes.len()].copy_from_slice(bytes);
}

/// Replaces a bounded narrow C string without clearing bytes after its
/// terminator.
#[doc(hidden)]
#[inline]
pub fn set_c_str(data: &mut [u8], value: &CStr) -> Result<(), StringMutationError> {
    preflight_c_str(data, value)?;
    commit_c_str(data, value);
    Ok(())
}

/// Preflights a bounded native-endian `u16` C string replacement without
/// touching storage.
#[doc(hidden)]
#[inline]
pub fn preflight_u16_c_str(data: &[u8], value: &U16CStr) -> Result<(), StringMutationError> {
    let capacity = wide_capacity(data)?;
    let units = value.as_slice_with_nul();
    if units.len() > capacity {
        return Err(StringMutationError::CapacityExceeded {
            length: units.len(),
            capacity,
        });
    }
    Ok(())
}

/// Commits a previously preflighted native-endian `u16` C string replacement.
#[doc(hidden)]
#[inline]
pub fn commit_u16_c_str(data: &mut [u8], value: &U16CStr) {
    let units = value.as_slice_with_nul();
    let active = core::mem::size_of_val(units);
    store_native_u16s(&mut data[..active], units);
}

/// Replaces a bounded native-endian `u16` C string without clearing units after
/// its terminator. `data` is wire bytes, never a mutable typed view.
#[doc(hidden)]
#[inline]
pub fn set_u16_c_str(data: &mut [u8], value: &U16CStr) -> Result<(), StringMutationError> {
    preflight_u16_c_str(data, value)?;
    commit_u16_c_str(data, value);
    Ok(())
}

/// Returns the UTF-8 source carried by a proof failure, when present.
#[doc(hidden)]
#[inline]
pub fn invalid_utf8_source(error: &StringProofError) -> Option<Utf8Error> {
    match error {
        StringProofError::InvalidUtf8(source) => Some(*source),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{BigU16, LittleU16, NativeU16, U8};

    #[test]
    fn proofs_ignore_unused_capacity_for_every_string_form() {
        assert_eq!(prove_str(&U8::new(2), b"hi\xff").unwrap(), "hi");
        assert_eq!(prove_c_str(b"a\0\xff").unwrap().to_bytes(), b"a");
        assert_eq!(
            prove_u16_str(&U8::new(1), &[0xd800, 9]).unwrap().as_slice(),
            &[0xd800]
        );
        assert_eq!(
            prove_u16_c_str(&[0xd800, 0, 7]).unwrap().as_slice(),
            &[0xd800]
        );

        assert_eq!(
            prove_str(&U8::new(3), b"hi").unwrap_err(),
            StringProofError::LengthOutOfBounds {
                length: 3,
                capacity: 2,
            }
        );
        assert!(matches!(
            prove_str(&U8::new(1), &[0xff]),
            Err(StringProofError::InvalidUtf8(_))
        ));
        assert_eq!(
            prove_c_str(b"none").unwrap_err(),
            StringProofError::MissingNul
        );
        assert_eq!(
            prove_u16_c_str(&[7, 8]).unwrap_err(),
            StringProofError::MissingNul
        );
    }

    #[test]
    fn narrow_mutations_preflight_and_preserve_unused_bytes() {
        let mut prefix = [0xa5];
        let mut str_data = [0xee; 4];
        set_str::<U8>(&mut prefix, &mut str_data, "hi").unwrap();
        assert_eq!(prefix, [2]);
        assert_eq!(str_data, [b'h', b'i', 0xee, 0xee]);

        let c_value = c"ok";
        let mut c_data = [0xee; 5];
        set_c_str(&mut c_data, c_value).unwrap();
        assert_eq!(c_data, [b'o', b'k', 0, 0xee, 0xee]);

        let before_prefix = prefix;
        let before_data = str_data;
        assert_eq!(
            set_str::<U8>(&mut prefix, &mut str_data, "toolong"),
            Err(StringMutationError::CapacityExceeded {
                length: 7,
                capacity: 4,
            })
        );
        assert_eq!(prefix, before_prefix);
        assert_eq!(str_data, before_data);
    }

    #[test]
    fn wide_mutations_use_native_units_and_preserve_unused_bytes() {
        let value = U16Str::from_slice(&[0xd800, 9]);
        let mut prefix = [0xa5; 2];
        let mut data = [0xee; 8];
        set_u16_str::<LittleU16>(&mut prefix, &mut data, value).unwrap();
        assert_eq!(prefix, [2, 0]);
        assert_eq!(&data[..2], &0xd800u16.to_ne_bytes());
        assert_eq!(&data[2..4], &9u16.to_ne_bytes());
        assert_eq!(&data[4..], &[0xee; 4]);

        let c_value = U16CStr::from_slice_truncate(&[7, 0]).unwrap();
        let mut c_data = [0xee; 6];
        set_u16_c_str(&mut c_data, c_value).unwrap();
        assert_eq!(&c_data[..2], &7u16.to_ne_bytes());
        assert_eq!(&c_data[2..4], &0u16.to_ne_bytes());
        assert_eq!(&c_data[4..], &[0xee; 2]);

        let before_prefix = prefix;
        let before_data = data;
        assert_eq!(
            set_u16_str::<NativeU16>(&mut prefix[..1], &mut data, value),
            Err(StringMutationError::PrefixSize {
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(prefix, before_prefix);
        assert_eq!(data, before_data);
    }

    #[test]
    fn representability_capacity_and_prefix_are_preflighted() {
        assert_eq!(
            preflight_length_prefixed::<U8>(256, 256, 1),
            Err(StringMutationError::LengthUnrepresentable { length: 256 })
        );
        assert_eq!(
            preflight_length_prefixed::<BigU16>(3, 2, 2),
            Err(StringMutationError::CapacityExceeded {
                length: 3,
                capacity: 2,
            })
        );
        assert_eq!(
            preflight_length_prefixed::<LittleU16>(1, 1, 1),
            Err(StringMutationError::PrefixSize {
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(
            set_u16_c_str(&mut [0; 3], U16CStr::from_slice_truncate(&[1, 0]).unwrap()),
            Err(StringMutationError::WideByteSize { actual: 3 })
        );
    }
}
