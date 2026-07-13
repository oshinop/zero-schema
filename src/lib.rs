#![doc = include_str!("../README.md")]
#![no_std]

extern crate self as zero_schema;

#[cfg(feature = "alloc")]
extern crate alloc;

mod codec;
mod decode;
mod encode;
mod error;
mod layout;
mod schema;
mod validation;
mod wire;

#[cfg(feature = "derive")]
pub use zero_schema_derive::ZeroSchema;

pub use decode::DecodeInput;
pub use encode::AlignedBytes;

/// Constructs zero-initialized, correctly aligned wire storage for a schema.
///
/// The returned [`AlignedBytes`] exposes exactly the schema's `WIRE_SIZE`
/// initialized bytes, while its address is aligned for the schema's wire type.
/// `Type` must be fully concrete: supply every type and const argument (and use
/// a concrete lifetime such as `'static` when a lifetime must be written).
///
/// This macro is hygienic and may be invoked when `zero-schema` is imported
/// under another name.
#[macro_export]
macro_rules! make_buffer_for {
    ($schema:ty) => {
        $crate::AlignedBytes::<
            <$schema as $crate::ZeroSchemaType>::Wire,
            { <$schema as $crate::ZeroSchemaType>::WIRE_SIZE },
        >::zeroed()
    };
}
#[cfg(feature = "alloc")]
pub use error::error_path_string;
pub use error::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError};
pub use layout::{
    ByteRange, Endian, EnumValueDescriptor, FieldDescriptor, FieldKind, IntegerRepr,
    LayoutDescriptor, LengthDescriptor, LengthRepr, PaddingPolicy, PrimitiveKind, StringDescriptor,
    StringEncoding, TailPolicy, TypeKind, VariantDescriptor,
};
pub use schema::{ScalarEnum, TaggedUnion, ZeroSchemaType};
pub use validation::{ValidationContext, ValidationFailure, ValidationOperation, ValidationResult};

#[doc(hidden)]
pub use zerocopy;

/// Unstable support surface consumed by generated code.
#[doc(hidden)]
pub mod __private {
    pub use crate::codec::*;
    pub use crate::decode::DecodeInput;
    pub use crate::encode::Prezeroed;
    pub use crate::error::__fmt_schema_error;
    pub use crate::schema::{
        __checked_wire_stride, __layout_constants_match, DecodeTaggedUnion, DecodeWire, EncodeWire,
        ScalarRepr, ScalarWire, decode_scalar, encode_scalar, read_scalar_raw, write_scalar_raw,
    };
    pub use crate::validation::{ValidationContext, ValidationOperation};
    pub use crate::wire::*;
    pub use crate::zerocopy;
}

#[cfg(test)]
mod tests {
    use core::marker::PhantomData;

    use super::*;

    #[derive(Debug)]
    struct TestError;

    impl core::fmt::Display for TestError {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("test error")
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Layout
        }

        fn schema(&self) -> &'static str {
            "Concrete"
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            None
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }

        fn __fmt_leaf(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("test error")
        }
    }

    struct GenericSchema<T>(PhantomData<T>);

    impl<T> ZeroSchemaType for GenericSchema<T> {
        type Wire = u32;
        type DecodeError = TestError;
        type EncodeError = TestError;

        const WIRE_SIZE: usize = core::mem::size_of::<Self::Wire>();
        const WIRE_ALIGN: usize = core::mem::align_of::<Self::Wire>();
        const WIRE_STRIDE: usize = core::mem::size_of::<Self::Wire>();
        const LAYOUT: &'static LayoutDescriptor = &LayoutDescriptor::__new(
            "GenericSchema",
            TypeKind::Struct,
            Self::WIRE_SIZE,
            Self::WIRE_ALIGN,
            Self::WIRE_STRIDE,
            PaddingPolicy::Ignore,
            &[],
            &[],
            &[],
            &[],
        );
    }

    #[test]
    fn make_buffer_for_accepts_a_fully_concrete_generic_schema() {
        let mut buffer = crate::make_buffer_for!(GenericSchema<u8>);
        assert_eq!(buffer.as_bytes(), &[0; 4]);
        buffer.as_bytes_mut()[0] = 7;
        assert_eq!(buffer.as_ref(), &[7, 0, 0, 0]);
        assert_eq!(
            (buffer.as_bytes().as_ptr() as usize) % GenericSchema::<u8>::WIRE_ALIGN,
            0
        );
    }
}
