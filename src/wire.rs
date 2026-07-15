//! All-bit-valid wire storage primitives.
//!
//! The generated schema implementation proves a complete root before exposing
//! a capability. These types only describe the in-memory representation and
//! provide bounded byte stores for already selected field slices.

use core::mem::{align_of, offset_of, size_of};

use zerocopy::{FromBytes, Immutable, KnownLayout};

use crate::error::LayoutError;

#[inline]
fn store_exact(destination: &mut [u8], source: &[u8]) -> Result<(), LayoutError> {
    if destination.len() != source.len() {
        return Err(LayoutError::IncorrectSize {
            expected: source.len(),
            actual: destination.len(),
        });
    }
    destination.copy_from_slice(source);
    Ok(())
}

macro_rules! native_wire {
    ($name:ident, $primitive:ty) => {
        #[doc(hidden)]
        #[repr(transparent)]
        #[derive(Clone, Copy, FromBytes, KnownLayout, Immutable)]
        pub struct $name(pub(crate) $primitive);

        impl $name {
            #[inline]
            pub const fn new(value: $primitive) -> Self {
                Self(value)
            }

            #[inline]
            pub const fn get(&self) -> $primitive {
                self.0
            }

            /// Stores this value into one exact-width native-endian field slice.
            #[inline]
            pub fn store(&self, destination: &mut [u8]) -> Result<(), LayoutError> {
                store_exact(destination, &self.0.to_ne_bytes())
            }

            /// Stores into an already preflighted exact-width selected slice.
            #[doc(hidden)]
            #[inline]
            pub fn store_preflighted(&self, destination: &mut [u8]) {
                destination.copy_from_slice(&self.0.to_ne_bytes());
            }
        }

        const _: () = {
            assert!(size_of::<$name>() == size_of::<$primitive>());
            assert!(align_of::<$name>() == align_of::<$primitive>());
        };
    };
}

macro_rules! explicit_wire {
    ($name:ident, $primitive:ty, $bytes:expr, $to:ident, $from:ident) => {
        #[doc(hidden)]
        #[repr(C)]
        #[derive(Clone, Copy, FromBytes, KnownLayout, Immutable)]
        pub struct $name {
            _align: [$primitive; 0],
            value: [u8; $bytes],
        }

        impl $name {
            #[inline]
            pub const fn new(value: $primitive) -> Self {
                Self {
                    _align: [],
                    value: value.$to(),
                }
            }

            #[inline]
            pub const fn get(&self) -> $primitive {
                <$primitive>::$from(self.value)
            }

            #[inline]
            pub const fn bytes(&self) -> &[u8; $bytes] {
                &self.value
            }

            /// Stores this value into one exact-width selected field slice.
            #[inline]
            pub fn store(&self, destination: &mut [u8]) -> Result<(), LayoutError> {
                store_exact(destination, &self.value)
            }

            /// Stores into an already preflighted exact-width selected slice.
            #[doc(hidden)]
            #[inline]
            pub fn store_preflighted(&self, destination: &mut [u8]) {
                destination.copy_from_slice(&self.value);
            }
        }

        const _: () = {
            assert!(offset_of!($name, _align) == 0);
            assert!(offset_of!($name, value) == 0);
            assert!(size_of::<$name>() == size_of::<$primitive>());
            assert!(align_of::<$name>() == align_of::<$primitive>());
        };
    };
}

native_wire!(U8, u8);
native_wire!(I8, i8);
native_wire!(NativeU16, u16);
native_wire!(NativeI16, i16);
native_wire!(NativeU32, u32);
native_wire!(NativeI32, i32);
native_wire!(NativeU64, u64);
native_wire!(NativeI64, i64);
native_wire!(NativeF32, f32);
native_wire!(NativeF64, f64);
explicit_wire!(LittleU16, u16, 2, to_le_bytes, from_le_bytes);
explicit_wire!(BigU16, u16, 2, to_be_bytes, from_be_bytes);
explicit_wire!(LittleI16, i16, 2, to_le_bytes, from_le_bytes);
explicit_wire!(BigI16, i16, 2, to_be_bytes, from_be_bytes);
explicit_wire!(LittleU32, u32, 4, to_le_bytes, from_le_bytes);
explicit_wire!(BigU32, u32, 4, to_be_bytes, from_be_bytes);
explicit_wire!(LittleI32, i32, 4, to_le_bytes, from_le_bytes);
explicit_wire!(BigI32, i32, 4, to_be_bytes, from_be_bytes);
explicit_wire!(LittleU64, u64, 8, to_le_bytes, from_le_bytes);
explicit_wire!(BigU64, u64, 8, to_be_bytes, from_be_bytes);
explicit_wire!(LittleI64, i64, 8, to_le_bytes, from_le_bytes);
explicit_wire!(BigI64, i64, 8, to_be_bytes, from_be_bytes);

macro_rules! float_wire {
    ($name:ident, $float:ty, $uint:ty, $bytes:expr, $to:ident, $from:ident) => {
        #[doc(hidden)]
        #[repr(C)]
        #[derive(Clone, Copy, FromBytes, KnownLayout, Immutable)]
        pub struct $name {
            _align: [$float; 0],
            value: [u8; $bytes],
        }

        impl $name {
            #[inline]
            pub const fn new(value: $float) -> Self {
                Self {
                    _align: [],
                    value: value.to_bits().$to(),
                }
            }

            #[inline]
            pub const fn get(&self) -> $float {
                <$float>::from_bits(<$uint>::$from(self.value))
            }

            #[inline]
            pub const fn bytes(&self) -> &[u8; $bytes] {
                &self.value
            }

            /// Stores this value into one exact-width selected field slice.
            #[inline]
            pub fn store(&self, destination: &mut [u8]) -> Result<(), LayoutError> {
                store_exact(destination, &self.value)
            }

            /// Stores into an already preflighted exact-width selected slice.
            #[doc(hidden)]
            #[inline]
            pub fn store_preflighted(&self, destination: &mut [u8]) {
                destination.copy_from_slice(&self.value);
            }
        }

        const _: () = {
            assert!(offset_of!($name, _align) == 0);
            assert!(offset_of!($name, value) == 0);
            assert!(size_of::<$name>() == size_of::<$float>());
            assert!(align_of::<$name>() == align_of::<$float>());
        };
    };
}

float_wire!(LittleF32, f32, u32, 4, to_le_bytes, from_le_bytes);
float_wire!(BigF32, f32, u32, 4, to_be_bytes, from_be_bytes);
float_wire!(LittleF64, f64, u64, 8, to_le_bytes, from_le_bytes);
float_wire!(BigF64, f64, u64, 8, to_be_bytes, from_be_bytes);

/// Raw one-byte Boolean representation.
#[doc(hidden)]
#[repr(transparent)]
#[derive(Clone, Copy, FromBytes, KnownLayout, Immutable)]
pub struct BoolWire(u8);

impl BoolWire {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(&self) -> u8 {
        self.0
    }

    /// Returns a logical Boolean only for the two declared wire encodings.
    #[inline]
    pub const fn decode(&self) -> Option<bool> {
        match self.0 {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    #[inline]
    pub const fn encode(value: bool) -> Self {
        Self(value as u8)
    }

    /// Stores a proven logical Boolean into one exact-width selected field slice.
    #[inline]
    pub fn store(value: bool, destination: &mut [u8]) -> Result<(), LayoutError> {
        store_exact(destination, &[value as u8])
    }

    /// Stores into an already preflighted exact-width selected slice.
    #[doc(hidden)]
    #[inline]
    pub fn store_preflighted(value: bool, destination: &mut [u8]) {
        destination.copy_from_slice(&[value as u8]);
    }
}

const _: () = {
    assert!(size_of::<BoolWire>() == size_of::<u8>());
    assert!(align_of::<BoolWire>() == align_of::<u8>());
};

/// Raw scalar-enum storage selected by its declared width and endian.
///
/// Generated scalar-enum support uses this trait to load and store only the
/// `u8`, `u16`, and `u32` representations accepted by the declaration grammar.
#[doc(hidden)]
pub trait ScalarWire: FromBytes + KnownLayout + Immutable + 'static {
    type Raw: Copy + Eq;

    const WIDTH: usize;

    fn load(&self) -> Self::Raw;
    fn store(value: Self::Raw, destination: &mut [u8]) -> Result<(), LayoutError>;

    /// Stores a value into an already preflighted exact-width selected slice.
    fn store_preflighted(value: Self::Raw, destination: &mut [u8]);
}

macro_rules! scalar_wire {
    ($wire:ty, $raw:ty) => {
        impl ScalarWire for $wire {
            type Raw = $raw;

            const WIDTH: usize = size_of::<$raw>();

            #[inline]
            fn load(&self) -> Self::Raw {
                self.get()
            }

            #[inline]
            fn store(value: Self::Raw, destination: &mut [u8]) -> Result<(), LayoutError> {
                <$wire>::new(value).store(destination)
            }

            #[inline]
            fn store_preflighted(value: Self::Raw, destination: &mut [u8]) {
                <$wire>::new(value).store_preflighted(destination);
            }
        }
    };
}

scalar_wire!(U8, u8);
scalar_wire!(NativeU16, u16);
scalar_wire!(LittleU16, u16);
scalar_wire!(BigU16, u16);
scalar_wire!(NativeU32, u32);
scalar_wire!(LittleU32, u32);
scalar_wire!(BigU32, u32);

/// Integer wire representation usable as a bounded string length prefix.
#[doc(hidden)]
pub trait LengthWire: FromBytes + KnownLayout + Immutable + 'static {
    /// Exact on-wire width of the prefix.
    const WIDTH: usize;

    /// Decodes the prefix without applying the string capacity bound.
    fn load(&self) -> Option<usize>;

    /// Returns whether this representation can encode `value`.
    fn represents(value: usize) -> bool;

    /// Stores one representable value into an exact-width selected prefix slice.
    fn store(value: usize, destination: &mut [u8]) -> Result<(), LayoutError>;

    /// Stores a representable value into an already preflighted exact prefix slice.
    fn store_preflighted(value: usize, destination: &mut [u8]);
}

macro_rules! length_wire {
    ($wire:ty, $primitive:ty) => {
        impl LengthWire for $wire {
            const WIDTH: usize = size_of::<$primitive>();

            #[inline]
            fn load(&self) -> Option<usize> {
                usize::try_from(self.get()).ok()
            }

            #[inline]
            fn represents(value: usize) -> bool {
                <$primitive>::try_from(value).is_ok()
            }

            #[inline]
            fn store(value: usize, destination: &mut [u8]) -> Result<(), LayoutError> {
                let value =
                    <$primitive>::try_from(value).map_err(|_| LayoutError::IncorrectSize {
                        expected: <$primitive>::MAX as usize,
                        actual: value,
                    })?;
                <$wire>::new(value).store(destination)
            }

            #[inline]
            fn store_preflighted(value: usize, destination: &mut [u8]) {
                <$wire>::new(value as $primitive).store_preflighted(destination);
            }
        }
    };
}

length_wire!(U8, u8);
length_wire!(NativeU16, u16);
length_wire!(LittleU16, u16);
length_wire!(BigU16, u16);
length_wire!(NativeU32, u32);
length_wire!(LittleU32, u32);
length_wire!(BigU32, u32);

/// Bounded UTF-8 string storage: a length prefix followed by `N` bytes.
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct StrWire<L, const N: usize> {
    len: L,
    data: [u8; N],
}

impl<L, const N: usize> StrWire<L, N> {
    pub const LEN_OFFSET: usize = offset_of!(Self, len);
    pub const DATA_OFFSET: usize = offset_of!(Self, data);

    #[inline]
    pub fn len_wire(&self) -> &L {
        &self.len
    }

    #[inline]
    pub fn data(&self) -> &[u8; N] {
        &self.data
    }
}

/// Fixed-capacity nul-terminated UTF-8 string storage.
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct CStrWire<const N: usize> {
    data: [u8; N],
}

impl<const N: usize> CStrWire<N> {
    pub const DATA_OFFSET: usize = offset_of!(Self, data);

    #[inline]
    pub fn data(&self) -> &[u8; N] {
        &self.data
    }
}

/// Bounded native-endian `u16` string storage: a length prefix and `N` units.
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct U16StrWire<L, const N: usize> {
    len: L,
    units: [u16; N],
}

impl<L, const N: usize> U16StrWire<L, N> {
    pub const LEN_OFFSET: usize = offset_of!(Self, len);
    pub const DATA_OFFSET: usize = offset_of!(Self, units);

    #[inline]
    pub fn len_wire(&self) -> &L {
        &self.len
    }

    #[inline]
    pub fn units(&self) -> &[u16; N] {
        &self.units
    }
}

/// Fixed-capacity nul-terminated native-endian `u16` string storage.
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct U16CStrWire<const N: usize> {
    units: [u16; N],
}

impl<const N: usize> U16CStrWire<N> {
    pub const DATA_OFFSET: usize = offset_of!(Self, units);

    #[inline]
    pub fn units(&self) -> &[u16; N] {
        &self.units
    }
}

macro_rules! alignment_marker {
    ($name:ident, $alignment:literal) => {
        #[doc(hidden)]
        #[repr(align($alignment))]
        #[derive(FromBytes, KnownLayout, Immutable)]
        pub struct $name;

        const _: () = {
            assert!(size_of::<$name>() == 0);
            assert!(align_of::<$name>() == $alignment);
        };
    };
}

alignment_marker!(Align1, 1);
alignment_marker!(Align2, 2);
alignment_marker!(Align4, 4);
alignment_marker!(Align8, 8);
alignment_marker!(Align16, 16);
alignment_marker!(Align32, 32);
alignment_marker!(Align64, 64);
alignment_marker!(Align128, 128);
alignment_marker!(Align256, 256);
alignment_marker!(Align512, 512);
alignment_marker!(Align1024, 1024);
alignment_marker!(Align2048, 2048);
alignment_marker!(Align4096, 4096);
alignment_marker!(Align8192, 8192);
alignment_marker!(Align16384, 16384);
alignment_marker!(Align32768, 32768);
alignment_marker!(Align65536, 65536);
alignment_marker!(Align131072, 131072);
alignment_marker!(Align262144, 262144);
alignment_marker!(Align524288, 524288);
alignment_marker!(Align1048576, 1_048_576);
alignment_marker!(Align2097152, 2_097_152);
alignment_marker!(Align4194304, 4_194_304);
alignment_marker!(Align8388608, 8_388_608);
alignment_marker!(Align16777216, 16_777_216);
alignment_marker!(Align33554432, 33_554_432);
alignment_marker!(Align67108864, 67_108_864);
alignment_marker!(Align134217728, 134_217_728);
alignment_marker!(Align268435456, 268_435_456);
alignment_marker!(Align536870912, 536_870_912);

/// Raises a field's alignment through a zero-sized marker type.
///
/// Macro output selects one `Align*` marker for each accepted `align = ...`
/// option. The wrapper owns no construction API because it is a wire-only
/// representation formed through checked immutable byte views.
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct AlignedWire<T, A> {
    _align: [A; 0],
    value: T,
}

impl<T, A> AlignedWire<T, A> {
    pub const VALUE_OFFSET: usize = offset_of!(Self, value);

    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! explicit {
        ($t:ty, $p:ty, $v:expr, $bytes:expr) => {{
            let wire = <$t>::new($v as $p);
            assert_eq!(wire.get(), $v as $p);
            assert_eq!(wire.bytes(), &$bytes);
            assert_eq!(size_of::<$t>(), size_of::<$p>());
            assert_eq!(align_of::<$t>(), align_of::<$p>());
        }};
    }

    #[test]
    fn explicit_endian_values_and_checked_stores() {
        explicit!(LittleU16, u16, 0x1234, [0x34, 0x12]);
        explicit!(BigU16, u16, 0x1234, [0x12, 0x34]);
        explicit!(LittleI16, i16, -2, [0xfe, 0xff]);
        explicit!(BigI16, i16, -2, [0xff, 0xfe]);
        explicit!(LittleU32, u32, 0x1234_5678, [0x78, 0x56, 0x34, 0x12]);
        explicit!(BigU32, u32, 0x1234_5678, [0x12, 0x34, 0x56, 0x78]);
        explicit!(LittleI32, i32, -2, [0xfe, 0xff, 0xff, 0xff]);
        explicit!(BigI32, i32, -2, [0xff, 0xff, 0xff, 0xfe]);
        explicit!(
            LittleU64,
            u64,
            0x0102_0304_0506_0708,
            [8, 7, 6, 5, 4, 3, 2, 1]
        );
        explicit!(BigU64, u64, 0x0102_0304_0506_0708, [1, 2, 3, 4, 5, 6, 7, 8]);

        let mut byte = [0; 1];
        <U8 as ScalarWire>::store(0x7f, &mut byte).unwrap();
        assert_eq!(byte, [0x7f]);

        let mut short = [0; 2];
        <NativeU16 as ScalarWire>::store(0x1234, &mut short).unwrap();
        assert_eq!(short, 0x1234u16.to_ne_bytes());
        <LittleU16 as ScalarWire>::store(0x1234, &mut short).unwrap();
        assert_eq!(short, [0x34, 0x12]);
        <BigU16 as ScalarWire>::store(0x1234, &mut short).unwrap();
        assert_eq!(short, [0x12, 0x34]);

        let mut scalar = [0; 4];
        <LittleU32 as ScalarWire>::store(0x0102_0304, &mut scalar).unwrap();
        assert_eq!(scalar, [4, 3, 2, 1]);
        <BigU32 as ScalarWire>::store(0x0102_0304, &mut scalar).unwrap();
        assert_eq!(scalar, [1, 2, 3, 4]);
        assert_eq!(
            <BigU16 as ScalarWire>::store(7, &mut scalar),
            Err(LayoutError::IncorrectSize {
                expected: 2,
                actual: 4,
            })
        );
    }

    #[test]
    fn native_float_bits_bool_and_length_stores() {
        for bits in [0u32, 0x8000_0000, 0x7fc0_1234] {
            let value = f32::from_bits(bits);
            assert_eq!(NativeF32::new(value).get().to_bits(), bits);
            assert_eq!(LittleF32::new(value).get().to_bits(), bits);
            assert_eq!(BigF32::new(value).get().to_bits(), bits);
        }
        for bits in [0u64, 0x8000_0000_0000_0000, 0x7ff8_0000_0000_1234] {
            let value = f64::from_bits(bits);
            assert_eq!(NativeF64::new(value).get().to_bits(), bits);
            assert_eq!(LittleF64::new(value).get().to_bits(), bits);
            assert_eq!(BigF64::new(value).get().to_bits(), bits);
        }

        assert_eq!(BoolWire::encode(false).decode(), Some(false));
        assert_eq!(BoolWire::encode(true).decode(), Some(true));
        assert_eq!(BoolWire::from_raw(2).decode(), None);
        let mut boolean = [0xa5];
        BoolWire::store(true, &mut boolean).unwrap();
        assert_eq!(boolean, [1]);

        let mut prefix = [0; 2];
        <LittleU16 as LengthWire>::store(0x1234, &mut prefix).unwrap();
        assert_eq!(prefix, [0x34, 0x12]);
        <BigU16 as LengthWire>::store(0x1234, &mut prefix).unwrap();
        assert_eq!(prefix, [0x12, 0x34]);
        assert!(!<U8 as LengthWire>::represents(256));
    }

    #[test]
    fn helper_layouts_and_alignment_wrappers_match_compiler_layout() {
        assert_eq!(StrWire::<LittleU32, 1>::LEN_OFFSET, 0);
        assert_eq!(StrWire::<LittleU32, 1>::DATA_OFFSET, 4);
        assert_eq!(size_of::<U16StrWire<U8, 1>>(), 4);
        assert_eq!(U16StrWire::<U8, 1>::DATA_OFFSET, 2);
        assert_eq!(size_of::<CStrWire<3>>(), 3);
        assert_eq!(align_of::<U16CStrWire<1>>(), align_of::<u16>());
        assert_eq!(align_of::<AlignedWire<u8, Align16>>(), 16);
        assert_eq!(size_of::<AlignedWire<u8, Align16>>(), 16);
        assert_eq!(AlignedWire::<u8, Align16>::VALUE_OFFSET, 0);
    }
}
