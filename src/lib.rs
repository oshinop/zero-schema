#![doc = include_str!("../README.md")]
#![no_std]

extern crate self as zero_schema;

#[cfg(feature = "alloc")]
extern crate alloc;

mod access;
mod array;
mod error;
mod layout;
mod mutation;
mod strings;
mod tagged;
mod wire;

pub use access::SchemaBuffer;
pub use array::{ArrayMut, ArrayMutIter, ArrayRef, ArrayRefIter};
#[cfg(feature = "alloc")]
pub use error::error_path_string;
pub use error::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError};
pub use layout::{
    ArrayDescriptor, ArrayElementKind, ByteRange, Endian, EnumValueDescriptor,
    ExternalTagDescriptor, FieldDescriptor, FieldKind, IntegerRepr, LayoutDescriptor,
    LengthDescriptor, LengthRepr, PrimitiveKind, StringDescriptor, StringEncoding, TypeKind,
    VariantDescriptor,
};
pub use mutation::{BytesMut, OptionMut, ScalarMut, StringMut};

/// Item-owning schema declaration attribute.
pub use zero_schema_macros::zero;

/// Names initialized receiving storage for one fully concrete root schema.
///
/// The resulting [`SchemaBuffer`] type is aligned for the root's opaque
/// generated wire projection and contains exactly the root wire's initialized
/// byte span. Supply every type and const argument, and use a concrete lifetime
/// (such as `'static`) when one must be written.
///
/// Construct the named type with [`SchemaBuffer::new`], [`Default::default`], or
/// [`make_schema_buffer!`]. With the `alloc` feature, `SchemaBuffer::new_boxed` and
/// `make_boxed_schema_buffer!` construct a `Box` of it directly on the heap. Initial
/// zeroes are Rust-memory initialization only; they do not imply that the bytes are
/// a valid instance of the root schema. [`make_zeroed_schema_buffer!`] (and, under
/// `alloc`, `make_zeroed_boxed_schema_buffer!`) are the constructors that additionally
/// require, at compile time, that the root's all-zero bytes are valid.
///
/// ```
/// # use zero_schema::{schema_buffer, zero};
/// # #[zero]
/// # struct Message { value: u32 }
/// # fn main() {
/// type MessageBuffer = schema_buffer!(Message);
/// let mut storage = MessageBuffer::new();
/// # storage.as_bytes_mut().copy_from_slice(&0_u32.to_ne_bytes());
/// let message = Message::access(storage.as_bytes()).unwrap();
/// assert_eq!(message.value(), 0);
/// # }
/// ```
#[macro_export]
macro_rules! schema_buffer {
    ($schema:ty) => {
        $crate::SchemaBuffer<
            <$schema as $crate::__private::WireType>::Wire,
            { <$schema as $crate::__private::WireType>::SIZE },
        >
    };
}

/// Creates initialized receiving storage for one fully concrete root schema.
///
/// This is the expression counterpart to [`schema_buffer!`]. The returned value
/// has type `schema_buffer!(Root)`. Populate its bytes from a producer, then call
/// `Root::access` or `Root::access_mut` to establish schema validity. For a root
/// whose all-zero bytes are a valid instance, [`make_zeroed_schema_buffer!`] states
/// and checks that property instead.
#[macro_export]
macro_rules! make_schema_buffer {
    ($schema:ty) => {{
        $crate::__assert_schema_buffer_root!($schema);
        <$crate::schema_buffer!($schema)>::new()
    }};
}

/// Creates initialized receiving storage for one fully concrete root schema
/// directly on the heap.
///
/// This is [`make_schema_buffer!`] with heap placement: the expression has type
/// `Box<schema_buffer!(Root)>` and is built through `SchemaBuffer::new_boxed`, so
/// no stack temporary of the root's size is formed. It has the same root
/// requirements and the same compile-time agreement checks, and the same meaning:
/// the bytes are initialized zeroes, not a valid instance of the root schema.
/// Populate them from a producer, then call `Root::access` or `Root::access_mut`.
///
/// Available with the `alloc` feature.
///
/// ```
/// # use zero_schema::zero;
/// # #[zero]
/// # struct Message { value: u32 }
/// # fn receive_producer_bytes(bytes: &mut [u8]) { bytes.copy_from_slice(&7_u32.to_ne_bytes()); }
/// # fn main() {
/// let mut slot = zero_schema::make_boxed_schema_buffer!(Message);
/// receive_producer_bytes(slot.as_bytes_mut());
/// let message = Message::access(slot.as_bytes()).unwrap();
/// assert_eq!(message.value(), 7);
/// # }
/// ```
#[cfg(feature = "alloc")]
#[macro_export]
macro_rules! make_boxed_schema_buffer {
    ($schema:ty) => {{
        $crate::__assert_schema_buffer_root!($schema);
        <$crate::schema_buffer!($schema)>::new_boxed()
    }};
}

/// Creates receiving storage that starts as a type-valid all-zero instance of one
/// fully concrete root schema.
///
/// This is [`make_schema_buffer!`] for a root whose `ZERO_IS_VALID` constant is
/// `true`. It applies the same fully-concrete-root checks and additionally fails
/// to compile, with a message naming the problem, when `Root::ZERO_IS_VALID` is
/// `false` (for example a scalar-enum field with no `= 0` discriminant; wrapping
/// that field in `Option<_>` makes the root zero-valid). The expression has type
/// `schema_buffer!(Root)`.
///
/// The result is type-valid until its bytes are changed: the first
/// `Root::access` or `Root::access_mut` succeeds and observes every field at its
/// zero value (zero numbers, `false`, empty strings, `None`, the zero-discriminant
/// enum variant or tagged-payload variant). Capability mutation preserves
/// validity; direct writes through [`SchemaBuffer::as_bytes_mut`] carry no such
/// promise. The buffer is still only storage: `access` and `access_mut` remain the
/// sole, eagerly proved route to a capability.
///
/// ```
/// # use zero_schema::{make_zeroed_schema_buffer, zero};
/// # #[zero]
/// # #[repr(u8)]
/// # #[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// # enum Mode { Idle = 0, Busy = 1 }
/// # #[zero]
/// # struct Status { mode: Mode, count: u32 }
/// # fn main() {
/// const { assert!(Status::ZERO_IS_VALID) };
/// let mut slot = make_zeroed_schema_buffer!(Status);
/// let mut status = Status::access_mut(slot.as_bytes_mut()).unwrap();
/// assert_eq!((status.mode(), status.count()), (Mode::Idle, 0));
/// status.count_mut().set(3).unwrap();
/// # }
/// ```
#[macro_export]
macro_rules! make_zeroed_schema_buffer {
    ($schema:ty) => {{
        $crate::__assert_zeroed_schema_buffer_root!($schema);
        <$crate::schema_buffer!($schema)>::new()
    }};
}

/// Creates a type-valid all-zero receiving buffer for one fully concrete root
/// schema directly on the heap.
///
/// This is [`make_zeroed_schema_buffer!`] with the heap placement of
/// [`make_boxed_schema_buffer!`]: the expression has type
/// `Box<schema_buffer!(Root)>`, no stack temporary of the root's size is formed,
/// and the same compile-time failure rejects a root whose `ZERO_IS_VALID` is
/// `false`. The first `Root::access` or `Root::access_mut` succeeds until the
/// bytes are changed.
///
/// Available with the `alloc` feature.
///
/// ```
/// # use zero_schema::zero;
/// # #[zero]
/// # struct Message { value: u32, flag: bool }
/// # fn main() {
/// let slot = zero_schema::make_zeroed_boxed_schema_buffer!(Message);
/// let message = Message::access(slot.as_bytes()).unwrap();
/// assert_eq!((message.value(), message.flag()), (0, false));
/// # }
/// ```
#[cfg(feature = "alloc")]
#[macro_export]
macro_rules! make_zeroed_boxed_schema_buffer {
    ($schema:ty) => {{
        $crate::__assert_zeroed_schema_buffer_root!($schema);
        <$crate::schema_buffer!($schema)>::new_boxed()
    }};
}

/// Compile-time agreement between a root's inherent layout constants and its
/// opaque wire projection, shared by the receiving-storage constructors.
#[doc(hidden)]
#[macro_export]
macro_rules! __assert_schema_buffer_root {
    ($schema:ty) => {
        const _: () = {
            assert!(<$schema>::SCHEMA_SIZE == <$schema as $crate::__private::WireType>::SIZE);
            assert!(<$schema>::SCHEMA_ALIGN == <$schema as $crate::__private::WireType>::ALIGN);
            assert!(<$schema>::SCHEMA_STRIDE == <$schema as $crate::__private::WireType>::STRIDE);
        };
    };
}

/// Root checks of [`make_schema_buffer!`] plus the compile-time requirement that an
/// all-zero span is a type-valid instance of the root.
#[doc(hidden)]
#[macro_export]
macro_rules! __assert_zeroed_schema_buffer_root {
    ($schema:ty) => {
        $crate::__assert_schema_buffer_root!($schema);
        const _: () = assert!(
            <$schema>::ZERO_IS_VALID,
            concat!(
                "`",
                stringify!($schema),
                "::ZERO_IS_VALID` is false, so a zero-start schema buffer would not be a valid root: ",
                "some field has no all-zero value (for example a scalar enum without a `= 0` variant, ",
                "or a tagged payload whose zero tag has no zero-valid variant). ",
                "Make that field an `Option<_>`, declare a zero variant, ",
                "or use `make_schema_buffer!` and fill the bytes from the producer before `access`."
            )
        );
    };
}

/// Unstable implementation surface consumed by generated code.
#[doc(hidden)]
pub mod __private;

#[cfg(test)]
mod tests {
    use core::marker::PhantomData;

    use super::*;

    #[repr(C, align(16))]
    #[derive(zerocopy::FromBytes, zerocopy::Immutable, zerocopy::KnownLayout)]
    struct GenericWire {
        bytes: [u8; 7],
    }

    struct GenericSchema<T>(PhantomData<T>);

    impl<T> GenericSchema<T> {
        const SCHEMA_SIZE: usize = core::mem::size_of::<GenericWire>();
        const SCHEMA_ALIGN: usize = core::mem::align_of::<GenericWire>();
        const SCHEMA_STRIDE: usize = core::mem::size_of::<GenericWire>();
    }

    static GENERIC_LAYOUT: LayoutDescriptor = LayoutDescriptor::__new(
        "GenericSchema",
        TypeKind::Struct,
        core::mem::size_of::<GenericWire>(),
        core::mem::align_of::<GenericWire>(),
        core::mem::size_of::<GenericWire>(),
        &[],
        &[],
        &[],
        &[],
    );

    static BORROWING_LAYOUT: LayoutDescriptor = LayoutDescriptor::__new(
        "BorrowingSchema",
        TypeKind::Struct,
        1,
        1,
        1,
        &[],
        &[],
        &[],
        &[],
    );

    impl<T> crate::__private::WireType for GenericSchema<T> {
        type Wire = GenericWire;

        const SIZE: usize = Self::SCHEMA_SIZE;
        const ALIGN: usize = Self::SCHEMA_ALIGN;
        const STRIDE: usize = Self::SCHEMA_STRIDE;
        const LAYOUT: &'static LayoutDescriptor = &GENERIC_LAYOUT;
    }

    #[derive(Debug)]
    struct CompositionError;

    impl core::fmt::Display for CompositionError {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("composition error")
        }
    }

    impl core::error::Error for CompositionError {}

    impl SchemaError for CompositionError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Layout
        }

        fn schema(&self) -> &'static str {
            "BorrowingSchema"
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            None
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }

        fn __fmt_leaf(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("composition error")
        }
    }

    struct CompositionOwner;

    impl crate::__private::OwnerAdapter for CompositionOwner {
        type AccessError = CompositionError;
        type MutationError = CompositionError;

        fn access_layout(_: LayoutError) -> Self::AccessError {
            CompositionError
        }

        fn mutation_layout(_: LayoutError) -> Self::MutationError {
            CompositionError
        }
    }

    struct BorrowingSupport;

    #[derive(Clone, Copy)]
    struct BorrowingToken;

    impl crate::__private::InputAccess for BorrowingSupport {
        type Token = BorrowingToken;
    }

    impl crate::__private::SchemaSupport for BorrowingSupport {
        type Wire = u8;
        type Owner = CompositionOwner;
        type Ref<'wire> = &'wire u8;
        type Mut<'wire> = ();

        fn validate<'wire>(
            _: crate::__private::SharedInput<'wire, Self::Wire>,
        ) -> Result<(), CompositionError> {
            Ok(())
        }

        fn make_ref<'wire>(
            proof: crate::__private::ProvedShared<'wire, Self, Self::Wire>,
        ) -> Self::Ref<'wire> {
            let input = proof.into_input(BorrowingToken);
            &input
                .subrange_bytes::<BorrowingSupport>(0, 1, BorrowingToken)
                .expect("exact byte input")[0]
        }

        fn make_mut<'wire>(_: crate::__private::ProvedExclusive<'wire, Self, Self::Wire>) {}
        fn input_token(_: &crate::__private::ExclusiveInput<'_, Self::Wire>) -> Self::Token {
            BorrowingToken
        }
    }

    struct BorrowingSchema<'source>(PhantomData<&'source u8>);

    impl crate::__private::WireType for BorrowingSchema<'_> {
        type Wire = u8;

        const SIZE: usize = 1;
        const ALIGN: usize = 1;
        const STRIDE: usize = 1;
        const LAYOUT: &'static LayoutDescriptor = &BORROWING_LAYOUT;
    }

    impl crate::__private::WireTypeSupport for BorrowingSchema<'_> {
        type Support = BorrowingSupport;
        type ZeroState = crate::__private::ZeroValid;
        const ZERO_IS_VALID: bool = true;
    }

    fn assert_lifetime_erased_support<'first, 'second>(
        _: PhantomData<&'first ()>,
        _: PhantomData<&'second ()>,
    ) {
        let _: PhantomData<
            <BorrowingSchema<'first> as crate::__private::WireTypeSupport>::Support,
        > = PhantomData::<<BorrowingSchema<'second> as crate::__private::WireTypeSupport>::Support>;
    }

    #[test]
    fn zero_state_or_normalizes_to_invalid_when_any_term_is_invalid() {
        type ValidThenInvalid = <crate::__private::ZeroValid as crate::__private::ZeroState>::Or<
            crate::__private::ZeroInvalid,
        >;
        type InvalidThenValid = <crate::__private::ZeroInvalid as crate::__private::ZeroState>::Or<
            crate::__private::ZeroValid,
        >;

        let _: PhantomData<crate::__private::ZeroInvalid> = PhantomData::<ValidThenInvalid>;
        let _: PhantomData<crate::__private::ZeroInvalid> = PhantomData::<InvalidThenValid>;
    }

    #[test]
    fn support_identity_erases_source_lifetimes_and_rebinds_logical_output() {
        assert_lifetime_erased_support(PhantomData, PhantomData);

        let bytes = [41_u8];
        let input = crate::__private::SharedInput::<u8>::from_checked(&bytes).unwrap();
        assert_eq!(input.read_copy::<u8>(0), Ok(41));
    }

    #[test]
    fn schema_buffer_is_initialized_storage() {
        let mut buffer = crate::make_schema_buffer!(GenericSchema<u8>);
        assert_eq!(buffer.as_bytes(), &[0; GenericSchema::<u8>::SCHEMA_SIZE]);
        assert_eq!(buffer.as_bytes().len(), GenericSchema::<u8>::SCHEMA_SIZE);
        assert_eq!(
            core::mem::align_of_val(&buffer),
            GenericSchema::<u8>::SCHEMA_ALIGN
        );
        assert_eq!(
            core::mem::size_of_val(&buffer),
            GenericSchema::<u8>::SCHEMA_STRIDE
        );
        assert_eq!(
            (buffer.as_bytes().as_ptr() as usize) % GenericSchema::<u8>::SCHEMA_ALIGN,
            0
        );

        buffer.as_bytes_mut()[0] = 7;
        assert_eq!(buffer.as_bytes()[0], 7);
    }
}
