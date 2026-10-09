//! Compile-time all-zero validity and the checked zero-start receiving buffers.
//!
//! `ZERO_IS_VALID` is exact, not conservative: for every schema below it must equal
//! whether the eager proof accepts an all-zero span. The agreement table checks the
//! constant against that proof; the remaining tests pin the checked constructors.

#![allow(dead_code)]

use core::ffi::CStr;

use widestring::{U16CStr, U16Str};
use zero_schema::{
    __private::{LogicalSchema, SchemaPatchType, WireTypeSupport},
    make_schema_buffer, make_zeroed_schema_buffer, schema_buffer, zero,
};
use zero_schema_cross_crate_child::{BigCode, ChildTag, DirectChild, OptionalChild};

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithZero {
    Idle = 0,
    Busy = 1,
}

#[zero]
#[repr(u16)]
#[zero(endian = "big")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BigWithZero {
    Zero = 0x0000,
    Other = 0x0102,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoZero {
    One = 1,
    Two = 2,
}

#[zero]
#[derive(Debug)]
pub struct PlainChild {
    pub id: u8,
    pub on: bool,
}

#[zero]
#[derive(Debug)]
pub struct NoZeroChild {
    pub code: NoZero,
}

#[zero]
pub struct Scalars<'b> {
    pub a: u8,
    pub b: i64,
    pub c: f32,
    pub d: f64,
    pub e: bool,
    pub bytes: &'b [u8; 4],
}

#[zero]
pub struct Strings<'b> {
    #[zero(capacity = 4, len_type = u16, endian = "big")]
    pub text: &'b str,
    #[zero(capacity = 3)]
    pub c_text: &'b CStr,
    #[zero(capacity = 2, len_type = u8)]
    pub wide: &'b U16Str,
    #[zero(capacity = 1)]
    pub wide_c: &'b U16CStr,
}

#[zero]
pub struct EnumField {
    pub mode: WithZero,
}

#[zero]
pub struct BigEnumField {
    pub mode: BigWithZero,
}

#[zero]
pub struct NoZeroField {
    pub mode: NoZero,
}

/// The documented fix: an enum without a zero variant is zero-valid once optional.
#[zero]
pub struct OptionalFix {
    pub mode: Option<NoZero>,
    pub child: Option<NoZeroChild>,
    pub list: Option<[NoZero; 2]>,
}

#[zero]
pub struct Nesting {
    pub fine: PlainChild,
    pub bad: NoZeroChild,
}

#[zero]
pub struct NestingFine {
    pub fine: PlainChild,
    pub more: EnumField,
}

#[zero]
pub struct ZeroArrays<'b> {
    pub modes: [WithZero; 2],
    pub grid: [[u8; 2]; 2],
    pub children: [[PlainChild; 2]; 2],
    #[zero(capacity = 3, len_type = u8)]
    pub names: [[&'b str; 2]; 2],
    #[zero(capacity = 2)]
    pub codes: [&'b CStr; 3],
}

#[zero]
pub struct InvalidEnumArray {
    pub modes: [NoZero; 2],
}

#[zero]
pub struct InvalidNestedArray {
    pub modes: [[NoZero; 2]; 3],
}

#[zero]
pub struct InvalidChildArray {
    pub children: [NoZeroChild; 2],
}

#[zero]
pub struct OptionalArray {
    pub modes: Option<[NoZero; 2]>,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Empty = 0,
    Data = 1,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoZeroKind {
    First = 1,
    Second = 2,
}

/// The zero tag selects a unit variant even though another variant is zero-invalid.
#[zero]
pub enum UnitAtZero {
    #[zero(tag = Kind::Empty)]
    Empty,
    #[zero(tag = Kind::Data)]
    Data(NoZeroChild),
}

#[zero]
pub enum ValidAtZero {
    #[zero(tag = Kind::Empty)]
    Empty(PlainChild),
    #[zero(tag = Kind::Data)]
    Data(NoZeroChild),
}

/// Another variant is zero-valid, but only the zero tag's variant counts.
#[zero]
pub enum InvalidAtZero {
    #[zero(tag = Kind::Empty)]
    Empty(NoZeroChild),
    #[zero(tag = Kind::Data)]
    Data(PlainChild),
}

/// The tag enum has a zero variant that the payload declaration does not claim.
#[zero]
pub enum UnclaimedZero {
    #[zero(tag = Kind::Data)]
    Data(PlainChild),
}

#[zero]
pub enum ByNoZeroKind {
    #[zero(tag = NoZeroKind::First)]
    First,
    #[zero(tag = NoZeroKind::Second)]
    Second(PlainChild),
}

#[zero]
pub struct TaggedUnitAtZero {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: UnitAtZero,
}

#[zero]
pub struct TaggedValidAtZero {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: ValidAtZero,
}

#[zero]
pub struct TaggedInvalidAtZero {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: InvalidAtZero,
}

#[zero]
pub struct TaggedUnclaimedZero {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: UnclaimedZero,
}

#[zero]
pub struct TaggedNoZeroKind {
    pub kind: NoZeroKind,
    #[zero(tag_field = kind)]
    pub payload: ByNoZeroKind,
}

#[zero]
pub struct Holder<T>
where
    T: WireTypeSupport
        + SchemaPatchType
        + for<'view> LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
{
    pub child: T,
}

#[zero]
pub struct ForeignZeroValid {
    pub direct: DirectChild,
    pub many: [DirectChild; 2],
    pub optional: Option<OptionalChild>,
}

#[zero]
pub struct ForeignInvalidRecord {
    pub child: OptionalChild,
}

#[zero]
pub struct ForeignInvalidEnum {
    pub code: BigCode,
}

#[zero]
pub struct ForeignTagged {
    pub tag: ChildTag,
    #[zero(tag_field = tag)]
    pub message: zero_schema_cross_crate_child::ChildMessage,
}

macro_rules! zero_validity {
    ($($schema:ty => $expected:expr),+ $(,)?) => {$({
        const _: () = assert!(<$schema>::ZERO_IS_VALID == $expected);
        let mut storage = make_schema_buffer!($schema);
        assert!(
            storage.as_bytes().iter().all(|byte| *byte == 0),
            "{} starts all-zero",
            stringify!($schema)
        );
        assert_eq!(
            <$schema>::access(storage.as_bytes()).is_ok(),
            $expected,
            "access of all-zero {}",
            stringify!($schema)
        );
        assert_eq!(
            <$schema>::access_mut(storage.as_bytes_mut()).is_ok(),
            $expected,
            "access_mut of all-zero {}",
            stringify!($schema)
        );
    })+};
}

#[test]
fn zero_is_valid_equals_the_eager_proof_of_an_all_zero_span() {
    zero_validity! {
        // Scalar-enum roots: a declared zero discriminant, whatever the endianness.
        WithZero => true,
        BigWithZero => true,
        NoZero => false,
        // Always-valid field kinds.
        Scalars<'static> => true,
        Strings<'static> => true,
        // Enum and nested-schema composition.
        EnumField => true,
        BigEnumField => true,
        NoZeroField => false,
        NoZeroChild => false,
        Nesting => false,
        NestingFine => true,
        // `Option<_>` over zero-invalid types is the fix.
        OptionalFix => true,
        OptionalArray => true,
        // Arrays take their element's value at every depth, strings included.
        ZeroArrays<'static> => true,
        InvalidEnumArray => false,
        InvalidNestedArray => false,
        InvalidChildArray => false,
        // Tagged unions need the zero tag's variant and its payload.
        TaggedUnitAtZero => true,
        TaggedValidAtZero => true,
        TaggedInvalidAtZero => false,
        TaggedUnclaimedZero => false,
        TaggedNoZeroKind => false,
        // Composition through a generic parameter and across crates.
        Holder<PlainChild> => true,
        Holder<NoZeroChild> => false,
        ForeignZeroValid => true,
        ForeignInvalidRecord => false,
        ForeignInvalidEnum => false,
        ForeignTagged => false,
    }
}

#[test]
fn zero_is_valid_is_usable_in_const_context_for_concrete_roots() {
    const ZERO_START: bool = NestingFine::ZERO_IS_VALID && !Nesting::ZERO_IS_VALID;
    const { assert!(ZERO_START) };
    const { assert!(Holder::<PlainChild>::ZERO_IS_VALID) };
    const { assert!(!Holder::<NoZeroChild>::ZERO_IS_VALID) };
    static ZERO_VALID_FLAGS: [bool; 2] = [WithZero::ZERO_IS_VALID, NoZero::ZERO_IS_VALID];
    assert_eq!(ZERO_VALID_FLAGS, [true, false]);
}

#[zero]
pub struct Everything<'b> {
    pub mode: WithZero,
    pub count: u32,
    #[zero(capacity = 4, len_type = u8)]
    pub name: &'b str,
    pub maybe: Option<NoZero>,
    pub modes: [WithZero; 2],
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub payload: ValidAtZero,
}

#[test]
fn zeroed_buffer_is_accepted_at_first_use_and_stays_valid_through_capabilities() {
    const { assert!(Everything::ZERO_IS_VALID) };
    let mut slot: schema_buffer!(Everything<'static>) =
        make_zeroed_schema_buffer!(Everything<'static>);
    assert_eq!(slot.as_bytes().len(), Everything::SCHEMA_SIZE);
    assert!(slot.as_bytes().iter().all(|byte| *byte == 0));

    {
        let view = Everything::access(slot.as_bytes()).expect("zero-start root is type-valid");
        assert_eq!(
            (view.mode(), view.count(), view.name(), view.maybe()),
            (WithZero::Idle, 0, "", None)
        );
        assert_eq!(view.modes().get(1), Some(WithZero::Idle));
        assert_eq!(view.payload().tag(), Kind::Empty);
        assert!(view.payload().empty().is_some());
    }
    {
        let mut view = Everything::access_mut(slot.as_bytes_mut())
            .expect("zero-start root is type-valid for exclusive access too");
        view.mode_mut().set(WithZero::Busy).unwrap();
        view.count_mut().set(7).unwrap();
        view.name_mut().set("abcd").unwrap();
        view.maybe_mut().set(Some(NoZero::Two)).unwrap();
    }

    // Capability mutation preserves type validity.
    let view = Everything::access(slot.as_bytes()).expect("still type-valid after mutation");
    assert_eq!(
        (view.mode(), view.count(), view.name(), view.maybe()),
        (WithZero::Busy, 7, "abcd", Some(NoZero::Two))
    );
}

#[test]
fn zeroed_buffer_for_a_concrete_generic_root_has_the_receiving_storage_type() {
    let slot: schema_buffer!(Holder<PlainChild>) = make_zeroed_schema_buffer!(Holder<PlainChild>);
    let view = Holder::<PlainChild>::access(slot.as_bytes()).unwrap();
    assert_eq!((view.child().id(), view.child().on()), (0, false));
}

#[test]
fn zeroed_buffer_accepts_scalar_enum_roots() {
    let slot = make_zeroed_schema_buffer!(BigWithZero);
    assert_eq!(
        BigWithZero::access(slot.as_bytes()).unwrap().get(),
        BigWithZero::Zero
    );
}

#[cfg(feature = "alloc")]
#[test]
fn boxed_zeroed_buffer_is_accepted_at_first_use() {
    const { assert!(Everything::ZERO_IS_VALID) };
    let mut slot: Box<schema_buffer!(Everything<'static>)> =
        zero_schema::make_zeroed_boxed_schema_buffer!(Everything<'static>);
    assert_eq!(slot.as_bytes().len(), Everything::SCHEMA_SIZE);
    assert!(Everything::access(slot.as_bytes()).is_ok());
    Everything::access_mut(slot.as_bytes_mut())
        .unwrap()
        .count_mut()
        .set(1)
        .unwrap();
    assert_eq!(Everything::access(slot.as_bytes()).unwrap().count(), 1);
}
