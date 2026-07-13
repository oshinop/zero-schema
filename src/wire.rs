use core::mem::{align_of, offset_of, size_of};

use zerocopy::{FromBytes, Immutable, KnownLayout};

macro_rules! native_wire {
    ($name:ident, $primitive:ty) => {
        #[doc(hidden)]
        #[repr(transparent)]
        #[derive(FromBytes, KnownLayout, Immutable)]
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
        }
    };
}

macro_rules! explicit_wire {
    ($name:ident, $primitive:ty, $bytes:expr, $to:ident, $from:ident) => {
        #[doc(hidden)]
        #[repr(C)]
        #[derive(FromBytes, KnownLayout, Immutable)]
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
    ($name:ident,$float:ty,$uint:ty,$bytes:expr,$to:ident,$from:ident) => {
        #[doc(hidden)]
        #[repr(C)]
        #[derive(FromBytes, KnownLayout, Immutable)]
        pub struct $name {
            _align: [$float; 0],
            value: [u8; $bytes],
        }
        impl $name {
            pub const fn new(value: $float) -> Self {
                Self {
                    _align: [],
                    value: value.to_bits().$to(),
                }
            }
            pub const fn get(&self) -> $float {
                <$float>::from_bits(<$uint>::$from(self.value))
            }
            pub const fn bytes(&self) -> &[u8; $bytes] {
                &self.value
            }
        }
        const _: () = {
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

#[doc(hidden)]
#[repr(transparent)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct BoolWire(u8);
impl BoolWire {
    #[inline]
    pub const fn raw(&self) -> u8 {
        self.0
    }
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
}

#[doc(hidden)]
pub trait LengthWire: FromBytes + KnownLayout + Immutable + 'static {
    fn to_usize(&self) -> Option<usize>;
    fn encoded(value: usize) -> Option<Self>
    where
        Self: Sized;
    fn write_to(
        &self,
        destination: &mut crate::encode::Prezeroed<'_>,
    ) -> Result<(), crate::error::LayoutError>;
}
macro_rules! length_wire {
    ($ty:ty,$int:ty) => {
        impl LengthWire for $ty {
            fn to_usize(&self) -> Option<usize> {
                usize::try_from(self.get()).ok()
            }
            fn encoded(value: usize) -> Option<Self> {
                <$int>::try_from(value).ok().map(Self::new)
            }
            fn write_to(
                &self,
                d: &mut crate::encode::Prezeroed<'_>,
            ) -> Result<(), crate::error::LayoutError> {
                d.write(0, &self.get().to_ne_bytes())
            }
        }
    };
}
length_wire!(U8, u8);
length_wire!(NativeU16, u16);
length_wire!(NativeU32, u32);
macro_rules! explicit_length {
    ($ty:ty,$int:ty) => {
        impl LengthWire for $ty {
            fn to_usize(&self) -> Option<usize> {
                usize::try_from(self.get()).ok()
            }
            fn encoded(value: usize) -> Option<Self> {
                <$int>::try_from(value).ok().map(Self::new)
            }
            fn write_to(
                &self,
                d: &mut crate::encode::Prezeroed<'_>,
            ) -> Result<(), crate::error::LayoutError> {
                d.write(0, self.bytes())
            }
        }
    };
}
explicit_length!(LittleU16, u16);
explicit_length!(BigU16, u16);
explicit_length!(LittleU32, u32);
explicit_length!(BigU32, u32);

#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct FixedStrWire<L, const N: usize> {
    len: L,
    data: [u8; N],
}
impl<L, const N: usize> FixedStrWire<L, N> {
    pub const LEN_OFFSET: usize = offset_of!(Self, len);
    pub const DATA_OFFSET: usize = offset_of!(Self, data);
    pub fn len_wire(&self) -> &L {
        &self.len
    }
    pub fn data(&self) -> &[u8; N] {
        &self.data
    }
}
#[doc(hidden)]
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable)]
pub struct FixedU16StrWire<L, const N: usize> {
    len: L,
    units: [u16; N],
}
impl<L, const N: usize> FixedU16StrWire<L, N> {
    pub const LEN_OFFSET: usize = offset_of!(Self, len);
    pub const DATA_OFFSET: usize = offset_of!(Self, units);
    pub fn len_wire(&self) -> &L {
        &self.len
    }
    pub fn units(&self) -> &[u16; N] {
        &self.units
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        layout::IntegerRepr,
        schema::{ScalarRepr, ScalarWire},
    };
    macro_rules! explicit {
        ($t:ty,$p:ty,$v:expr,$bytes:expr) => {{
            let w = <$t>::new($v as $p);
            assert_eq!(w.get(), $v as $p);
            assert_eq!(w.bytes(), &$bytes);
            assert_eq!(size_of::<$t>(), size_of::<$p>());
            assert_eq!(align_of::<$t>(), align_of::<$p>());
        }};
    }
    #[test]
    fn every_explicit_codec() {
        explicit!(LittleU16, u16, 0x1234, [0x34, 0x12]);
        explicit!(BigU16, u16, 0x1234, [0x12, 0x34]);
        explicit!(LittleI16, i16, -2, [0xfe, 0xff]);
        explicit!(BigI16, i16, -2, [0xff, 0xfe]);
        explicit!(LittleU32, u32, 0x12345678, [0x78, 0x56, 0x34, 0x12]);
        explicit!(BigU32, u32, 0x12345678, [0x12, 0x34, 0x56, 0x78]);
        explicit!(LittleI32, i32, -2, [0xfe, 0xff, 0xff, 0xff]);
        explicit!(BigI32, i32, -2, [0xff, 0xff, 0xff, 0xfe]);
        explicit!(LittleU64, u64, 0x0102030405060708, [8, 7, 6, 5, 4, 3, 2, 1]);
        explicit!(BigU64, u64, 0x0102030405060708, [1, 2, 3, 4, 5, 6, 7, 8]);
        explicit!(
            LittleI64,
            i64,
            -2,
            [0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        explicit!(
            BigI64,
            i64,
            -2,
            [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe]
        );
    }
    #[test]
    fn native_and_float_bits() {
        assert_eq!(NativeU16::new(7).get(), 7);
        assert_eq!(NativeI16::new(-7).get(), -7);
        assert_eq!(NativeU32::new(7).get(), 7);
        assert_eq!(NativeI32::new(-7).get(), -7);
        assert_eq!(NativeU64::new(7).get(), 7);
        assert_eq!(NativeI64::new(-7).get(), -7);
        for bits in [0u32, 0x8000_0000, 0x7fc0_1234] {
            let value = f32::from_bits(bits);
            assert_eq!(NativeF32::new(value).get().to_bits(), bits);
            assert_eq!(LittleF32::new(value).get().to_bits(), bits);
            assert_eq!(BigF32::new(value).get().to_bits(), bits);
            assert_eq!(LittleF32::new(value).bytes(), &bits.to_le_bytes());
            assert_eq!(BigF32::new(value).bytes(), &bits.to_be_bytes());
        }
        for bits in [0u64, 0x8000_0000_0000_0000, 0x7ff8_0000_0000_1234] {
            let value = f64::from_bits(bits);
            assert_eq!(NativeF64::new(value).get().to_bits(), bits);
            assert_eq!(LittleF64::new(value).get().to_bits(), bits);
            assert_eq!(BigF64::new(value).get().to_bits(), bits);
            assert_eq!(LittleF64::new(value).bytes(), &bits.to_le_bytes());
            assert_eq!(BigF64::new(value).bytes(), &bits.to_be_bytes());
        }
        assert_eq!(align_of::<LittleF32>(), align_of::<f32>());
        assert_eq!(align_of::<BigF32>(), align_of::<f32>());
        assert_eq!(align_of::<LittleF64>(), align_of::<f64>());
        assert_eq!(align_of::<BigF64>(), align_of::<f64>());
    }
    #[test]
    fn bool_domain() {
        assert_eq!(BoolWire::encode(false).decode(), Some(false));
        assert_eq!(BoolWire::encode(true).decode(), Some(true));
        assert_eq!(BoolWire(2).decode(), None);
    }
    #[test]
    fn scalar_widths() {
        assert_eq!(<u8 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U8);
        assert_eq!(<u16 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U16);
        assert_eq!(<u32 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U32);
        assert_eq!(<U8 as ScalarWire>::read(&U8::new(9)), 9);
        assert_eq!(
            <LittleU16 as ScalarWire>::read(&LittleU16::new(0x1234)),
            0x1234
        );
        assert_eq!(
            <BigU32 as ScalarWire>::read(&BigU32::new(0x12345678)),
            0x12345678
        );
    }
    #[test]
    fn helper_layout() {
        assert_eq!(FixedStrWire::<LittleU32, 1>::LEN_OFFSET, 0);
        assert_eq!(FixedStrWire::<LittleU32, 1>::DATA_OFFSET, 4);
        assert_eq!(size_of::<FixedU16StrWire<U8, 1>>(), 4);
        assert_eq!(FixedU16StrWire::<U8, 1>::DATA_OFFSET, 2);
    }
}
