use core::mem::{align_of, size_of};

use crate::decode::DecodeInput;
use crate::encode::Prezeroed;
use crate::error::{LayoutError, SchemaError};
use crate::layout::{IntegerRepr, LayoutDescriptor};
use crate::wire::{BigU16, BigU32, LittleU16, LittleU32, NativeU16, NativeU32, U8};
use crate::zerocopy::{FromBytes, Immutable, KnownLayout};

/// A type with a fixed, all-bit-valid wire representation.
pub trait ZeroSchemaType: Sized {
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type DecodeError: SchemaError + 'static;
    type EncodeError: SchemaError + 'static;

    const WIRE_SIZE: usize;
    const WIRE_ALIGN: usize;
    const WIRE_STRIDE: usize;
    const LAYOUT: &'static LayoutDescriptor;
}

/// Cross-crate decoding entry point used by generated implementations.
#[doc(hidden)]
pub trait DecodeWire<'src>: ZeroSchemaType {
    fn decode_at(input: DecodeInput<'src, Self::Wire>) -> Result<Self, Self::DecodeError>;
}

/// Cross-crate encoding entry point used by generated implementations.
#[doc(hidden)]
pub trait EncodeWire: ZeroSchemaType {
    fn validate_encode(&self) -> Result<(), Self::EncodeError>;
    fn encode_at(&self, destination: &mut Prezeroed<'_>) -> Result<(), Self::EncodeError>;
}

mod sealed {
    pub trait ScalarRepr {}
    pub trait ScalarWire {}

    impl ScalarRepr for u8 {}
    impl ScalarRepr for u16 {}
    impl ScalarRepr for u32 {}

    impl ScalarWire for super::U8 {}
    impl ScalarWire for super::NativeU16 {}
    impl ScalarWire for super::LittleU16 {}
    impl ScalarWire for super::BigU16 {}
    impl ScalarWire for super::NativeU32 {}
    impl ScalarWire for super::LittleU32 {}
    impl ScalarWire for super::BigU32 {}
}

/// The closed set of integer representations available to scalar schemas.
#[doc(hidden)]
#[allow(private_bounds)]
pub trait ScalarRepr: sealed::ScalarRepr + Copy + Eq + Into<u64> {
    const INTEGER_REPR: IntegerRepr;
}

impl ScalarRepr for u8 {
    const INTEGER_REPR: IntegerRepr = IntegerRepr::U8;
}
impl ScalarRepr for u16 {
    const INTEGER_REPR: IntegerRepr = IntegerRepr::U16;
}
impl ScalarRepr for u32 {
    const INTEGER_REPR: IntegerRepr = IntegerRepr::U32;
}

/// The closed set of endian-aware scalar wire codecs.
#[doc(hidden)]
#[allow(private_bounds)]
pub trait ScalarWire: sealed::ScalarWire + FromBytes + KnownLayout + Immutable + 'static {
    type Repr: ScalarRepr;

    fn read(&self) -> Self::Repr;
    fn write(value: Self::Repr, destination: &mut Prezeroed<'_>) -> Result<(), LayoutError>;
}

macro_rules! scalar_wire {
    ($wire:ty, $repr:ty, $bytes:ident) => {
        impl ScalarWire for $wire {
            type Repr = $repr;

            #[inline]
            fn read(&self) -> Self::Repr {
                self.get()
            }

            #[inline]
            fn write(
                value: Self::Repr,
                destination: &mut Prezeroed<'_>,
            ) -> Result<(), LayoutError> {
                destination.write(0, &value.$bytes())
            }
        }
    };
}

scalar_wire!(U8, u8, to_ne_bytes);
scalar_wire!(NativeU16, u16, to_ne_bytes);
scalar_wire!(LittleU16, u16, to_le_bytes);
scalar_wire!(BigU16, u16, to_be_bytes);
scalar_wire!(NativeU32, u32, to_ne_bytes);
scalar_wire!(LittleU32, u32, to_le_bytes);
scalar_wire!(BigU32, u32, to_be_bytes);

pub trait ScalarEnum: ZeroSchemaType<Wire: ScalarWire> {
    fn from_raw(raw: <Self::Wire as ScalarWire>::Repr) -> Option<Self>;
    fn to_raw(&self) -> <Self::Wire as ScalarWire>::Repr;

    #[doc(hidden)]
    fn __unknown(raw: <Self::Wire as ScalarWire>::Repr) -> Self::DecodeError;
    #[doc(hidden)]
    fn __decode_layout(error: LayoutError) -> Self::DecodeError;
    #[doc(hidden)]
    fn __encode_layout(error: LayoutError) -> Self::EncodeError;
}

pub trait TaggedUnion: EncodeWire {
    type Tag: ScalarEnum;
    type PayloadWire: FromBytes + KnownLayout + Immutable + 'static;

    fn tag(&self) -> Self::Tag;
    fn validate_payload_encode(&self) -> Result<(), Self::EncodeError>;
    fn encode_payload_at(&self, destination: &mut Prezeroed<'_>) -> Result<(), Self::EncodeError>;
}

pub trait DecodeTaggedUnion<'src>: TaggedUnion + DecodeWire<'src> {
    fn decode_payload(
        tag: &Self::Tag,
        input: DecodeInput<'src, Self::PayloadWire>,
    ) -> Result<Self, Self::DecodeError>;
    fn validate_decoded(&self) -> Result<(), Self::DecodeError>;
}

#[doc(hidden)]
#[inline]
pub fn decode_scalar<E: ScalarEnum>(input: DecodeInput<'_, E::Wire>) -> Result<E, E::DecodeError> {
    let raw = input.wire().read();
    E::from_raw(raw).ok_or_else(|| E::__unknown(raw))
}

#[doc(hidden)]
#[inline]
pub fn encode_scalar<E: ScalarEnum>(
    value: &E,
    destination: &mut Prezeroed<'_>,
) -> Result<(), E::EncodeError> {
    E::Wire::write(value.to_raw(), destination).map_err(E::__encode_layout)
}

#[doc(hidden)]
#[inline]
pub fn read_scalar_raw<E: ScalarEnum>(wire: &E::Wire) -> <E::Wire as ScalarWire>::Repr {
    wire.read()
}

#[doc(hidden)]
#[inline]
pub fn write_scalar_raw<E: ScalarEnum>(
    raw: <E::Wire as ScalarWire>::Repr,
    destination: &mut Prezeroed<'_>,
) -> Result<(), LayoutError> {
    E::Wire::write(raw, destination)
}

/// Computes the size of one aligned wire slot, returning `None` on overflow.
#[doc(hidden)]
pub const fn __checked_wire_stride(size: usize, align: usize) -> Option<usize> {
    if align == 0 || !align.is_power_of_two() {
        return None;
    }
    match size.checked_add(align - 1) {
        Some(rounded) => Some(rounded & !(align - 1)),
        None => None,
    }
}

/// Verifies the three layout constants generated for `T`.
#[doc(hidden)]
pub const fn __layout_constants_match<T: ZeroSchemaType>() -> bool {
    T::WIRE_SIZE == size_of::<T::Wire>()
        && T::WIRE_ALIGN == align_of::<T::Wire>()
        && match __checked_wire_stride(T::WIRE_SIZE, T::WIRE_ALIGN) {
            Some(stride) => T::WIRE_STRIDE == stride,
            None => false,
        }
}

#[cfg(test)]
mod tests {
    use core::fmt;

    use super::*;
    use crate::error::{ErrorKind, ErrorPathSegment};
    use crate::layout::{Endian, PaddingPolicy, TypeKind};

    #[derive(Debug)]
    enum TestError {
        Layout(LayoutError),
        Unknown(u8),
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Layout(error) => error.fmt(f),
                Self::Unknown(value) => write!(f, "unknown {value}"),
            }
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            match self {
                Self::Layout(_) => ErrorKind::Layout,
                Self::Unknown(_) => ErrorKind::UnknownEnumValue,
            }
        }
        fn schema(&self) -> &'static str {
            "TestScalar"
        }
        fn segment(&self) -> Option<ErrorPathSegment> {
            None
        }
        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }
        fn __fmt_leaf(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    static TEST_LAYOUT: LayoutDescriptor = LayoutDescriptor::__new(
        "TestScalar",
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U8,
            endian: Endian::Native,
        },
        1,
        1,
        1,
        PaddingPolicy::Ignore,
        &[],
        &[],
        &[],
        &[],
    );

    #[derive(Debug, Eq, PartialEq)]
    struct TestScalar(u8);

    impl ZeroSchemaType for TestScalar {
        type Wire = U8;
        type DecodeError = TestError;
        type EncodeError = TestError;
        const WIRE_SIZE: usize = 1;
        const WIRE_ALIGN: usize = 1;
        const WIRE_STRIDE: usize = 1;
        const LAYOUT: &'static LayoutDescriptor = &TEST_LAYOUT;
    }

    impl ScalarEnum for TestScalar {
        fn from_raw(raw: u8) -> Option<Self> {
            (raw <= 1).then_some(Self(raw))
        }
        fn to_raw(&self) -> u8 {
            self.0
        }
        fn __unknown(raw: u8) -> TestError {
            TestError::Unknown(raw)
        }
        fn __decode_layout(error: LayoutError) -> TestError {
            TestError::Layout(error)
        }
        fn __encode_layout(error: LayoutError) -> TestError {
            TestError::Layout(error)
        }
    }

    fn generic_decode<E: ScalarEnum>(bytes: &[u8]) -> Result<E, E::DecodeError> {
        decode_scalar::<E>(DecodeInput::from_exact(bytes).map_err(E::__decode_layout)?)
    }

    fn generic_encode<E: ScalarEnum>(value: &E, bytes: &mut [u8]) -> Result<(), E::EncodeError> {
        let mut destination = Prezeroed::new(bytes);
        encode_scalar(value, &mut destination)
    }

    #[test]
    fn generic_scalar_codec_preserves_domain_and_bytes() {
        let mut bytes = [1_u8];
        let value = generic_decode::<TestScalar>(&bytes).unwrap();
        generic_encode(&value, &mut bytes).unwrap();
        assert_eq!(value, TestScalar(1));
        assert_eq!(bytes, [1]);
        let error =
            decode_scalar::<TestScalar>(DecodeInput::from_exact(&[7]).unwrap()).unwrap_err();
        assert!(matches!(error, TestError::Unknown(7)));
    }

    #[test]
    fn sealed_wire_codecs_emit_exact_endian_bytes() {
        let mut bytes = [0xff; 4];
        let mut destination = Prezeroed::new(&mut bytes);
        <LittleU32 as ScalarWire>::write(0x0102_0304, &mut destination).unwrap();
        assert_eq!(bytes, [4, 3, 2, 1]);
        assert_eq!(<BigU16 as ScalarWire>::Repr::INTEGER_REPR, IntegerRepr::U16);
    }

    #[test]
    fn checked_stride_and_declared_layout_agree() {
        assert_eq!(__checked_wire_stride(5, 4), Some(8));
        assert_eq!(__checked_wire_stride(usize::MAX, 2), None);
        assert_eq!(__checked_wire_stride(1, 0), None);
        assert!(__layout_constants_match::<TestScalar>());
    }

    struct ManualTagged(TestScalar);

    impl ZeroSchemaType for ManualTagged {
        type Wire = U8;
        type DecodeError = TestError;
        type EncodeError = TestError;
        const WIRE_SIZE: usize = 1;
        const WIRE_ALIGN: usize = 1;
        const WIRE_STRIDE: usize = 1;
        const LAYOUT: &'static LayoutDescriptor = &TEST_LAYOUT;
    }
    impl EncodeWire for ManualTagged {
        fn validate_encode(&self) -> Result<(), TestError> {
            Ok(())
        }
        fn encode_at(&self, destination: &mut Prezeroed<'_>) -> Result<(), TestError> {
            encode_scalar(&self.0, destination)
        }
    }
    impl TaggedUnion for ManualTagged {
        type Tag = TestScalar;
        type PayloadWire = [u8; 0];
        fn tag(&self) -> Self::Tag {
            TestScalar(self.0.0)
        }
        fn validate_payload_encode(&self) -> Result<(), TestError> {
            Ok(())
        }
        fn encode_payload_at(&self, _: &mut Prezeroed<'_>) -> Result<(), TestError> {
            Ok(())
        }
    }

    #[test]
    fn manual_tagged_union_projections_are_generic() {
        fn probe<T: TaggedUnion>(value: &T) {
            let tag = value.tag();
            let _: <<T::Tag as ZeroSchemaType>::Wire as ScalarWire>::Repr = tag.to_raw();
        }
        probe(&ManualTagged(TestScalar(1)));
    }

    struct LifetimeTagged<'other, 'borrow> {
        borrowed: &'borrow u8,
        _other: core::marker::PhantomData<&'other u8>,
    }

    impl<'other, 'borrow> ZeroSchemaType for LifetimeTagged<'other, 'borrow> {
        type Wire = U8;
        type DecodeError = TestError;
        type EncodeError = TestError;
        const WIRE_SIZE: usize = 1;
        const WIRE_ALIGN: usize = 1;
        const WIRE_STRIDE: usize = 1;
        const LAYOUT: &'static LayoutDescriptor = &TEST_LAYOUT;
    }

    impl<'input, 'borrow, 'other> DecodeWire<'input> for LifetimeTagged<'other, 'borrow>
    where
        'input: 'borrow,
        'borrow: 'other,
    {
        fn decode_at(input: DecodeInput<'input, U8>) -> Result<Self, TestError> {
            Ok(Self {
                borrowed: &input.bytes()[0],
                _other: core::marker::PhantomData,
            })
        }
    }

    impl<'other, 'borrow> EncodeWire for LifetimeTagged<'other, 'borrow> {
        fn validate_encode(&self) -> Result<(), TestError> {
            Ok(())
        }
        fn encode_at(&self, destination: &mut Prezeroed<'_>) -> Result<(), TestError> {
            <U8 as ScalarWire>::write(*self.borrowed, destination).map_err(TestError::Layout)
        }
    }

    impl<'other, 'borrow> TaggedUnion for LifetimeTagged<'other, 'borrow> {
        type Tag = TestScalar;
        type PayloadWire = [u8; 0];
        fn tag(&self) -> TestScalar {
            TestScalar(*self.borrowed)
        }
        fn validate_payload_encode(&self) -> Result<(), TestError> {
            Ok(())
        }
        fn encode_payload_at(&self, _: &mut Prezeroed<'_>) -> Result<(), TestError> {
            Ok(())
        }
    }

    fn encode_with_unrelated_lifetimes<'a, 'b>(value: &LifetimeTagged<'a, 'b>) -> u8 {
        let mut bytes = [0_u8];
        value.validate_encode().unwrap();
        value.encode_at(&mut Prezeroed::new(&mut bytes)).unwrap();
        bytes[0]
    }

    fn decode_from_longer<'input, 'borrow, 'other>(
        input: DecodeInput<'input, U8>,
    ) -> Result<LifetimeTagged<'other, 'borrow>, TestError>
    where
        'input: 'borrow,
        'borrow: 'other,
    {
        DecodeWire::decode_at(input)
    }

    #[test]
    fn decode_lifetime_chain_and_encode_independence() {
        fn projections<T>(value: &T)
        where
            T: TaggedUnion<Tag = TestScalar, PayloadWire = [u8; 0]>,
        {
            let raw: <<T::Tag as ZeroSchemaType>::Wire as ScalarWire>::Repr = value.tag().to_raw();
            let numeric: u64 = raw.into();
            assert_eq!(numeric, 1);
        }

        let bytes = [1_u8];
        let input = DecodeInput::from_exact(&bytes).unwrap();
        let decoded: LifetimeTagged<'_, '_> = decode_from_longer(input).unwrap();
        assert!(core::ptr::eq(decoded.borrowed, &bytes[0]));
        assert_eq!(encode_with_unrelated_lifetimes(&decoded), 1);
        projections(&decoded);
    }
}
