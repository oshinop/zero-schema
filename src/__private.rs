//! Audited implementation surface consumed by generated code.
//!
//! This module is intentionally explicit: adding a generated-code dependency
//! requires naming it here rather than exposing a whole runtime module.

use crate::{
    error::{LayoutError, SchemaError},
    layout::LayoutDescriptor,
};
use zerocopy::{FromBytes, Immutable, KnownLayout};
/// Generated capability marker required for token-gated bounded byte access.
///
/// Every macro-generated support module supplies a distinct public token type
/// whose private field prevents downstream construction. These doc-hidden
/// implementation traits are macro contracts, not a user extension safety
/// boundary: handwritten implementations are outside the generated-capability
/// safety promise.
#[doc(hidden)]
pub trait InputAccess {
    type Token: Copy;
}
/// Authorization associated with one generated root wire projection.
///
/// Macro output implements this exactly once for its private physical wire and
/// uses the generated support module's private token. This remains unsealed so
/// independently generated schemas can compose across crate boundaries.
#[doc(hidden)]
pub trait RootInputAccess {
    type Token: Copy;
}

impl<T, A> RootInputAccess for crate::wire::AlignedWire<T, A>
where
    T: RootInputAccess,
{
    type Token = T::Token;
}

/// Opaque compiler-generated wire projection for a logical schema.
///
/// This is a safe, doc-hidden composition contract for macro output. It is not
/// a user extension API and no public capability exposes `Wire` values or
/// references. The constants must describe the compiler-derived wire layout.
#[doc(hidden)]
pub trait WireType {
    /// Private physical wire form used only to carry alignment and proof bounds.
    type Wire: FromBytes + KnownLayout + Immutable + 'static;

    /// Exact compiler-derived wire size.
    const SIZE: usize;

    /// Compiler-derived wire alignment.
    const ALIGN: usize;

    /// Compiler-derived slot stride.
    const STRIDE: usize;

    /// Diagnostic metadata for the wire layout.
    const LAYOUT: &'static LayoutDescriptor;
}

mod zero_state_sealed {
    pub trait Sealed {}
}

/// Type-level classification of whether an all-zero wire representation is
/// logically valid.
///
/// This is sealed so generated code can compose states but cannot define a
/// third state outside the protocol.
#[doc(hidden)]
pub trait ZeroState: zero_state_sealed::Sealed {
    type Or<Rhs: ZeroState>: ZeroState;
    /// Invalid only when both operands are invalid.
    type And<Rhs: ZeroState>: ZeroState;
}

/// An all-zero wire representation can encode a valid logical value.
#[doc(hidden)]
pub enum ZeroValid {}

/// An all-zero wire representation cannot encode a valid logical value.
#[doc(hidden)]
pub enum ZeroInvalid {}

impl zero_state_sealed::Sealed for ZeroValid {}
impl zero_state_sealed::Sealed for ZeroInvalid {}

impl ZeroState for ZeroValid {
    type Or<Rhs: ZeroState> = Rhs;
    type And<Rhs: ZeroState> = ZeroValid;
}

impl ZeroState for ZeroInvalid {
    type Or<Rhs: ZeroState> = ZeroInvalid;
    type And<Rhs: ZeroState> = Rhs;
}

/// Refinement for logical schema types that can use the all-zero optional
/// sentinel without colliding with a valid value.
#[doc(hidden)]
pub trait OptionalWireType: WireTypeSupport<ZeroState = ZeroInvalid> {}

impl<T> OptionalWireType for T where T: WireTypeSupport<ZeroState = ZeroInvalid> {}

/// Binds a logical declaration to source-lifetime-erased generated support.
///
/// This deliberately extends, rather than changes, `WireType`: wire-only
/// Phase 3 output remains usable while the capability emitter adds this impl.
/// For every `Message<'source>`, `Support` contains no declaration lifetime;
/// its GATs rebind logical and capability output to the checked input lifetime.
#[doc(hidden)]
pub trait WireTypeSupport: WireType {
    type Support: SchemaSupport<Wire = Self::Wire> + 'static;
    type ZeroState: ZeroState;
}

/// Generated support that also has a concrete logical patch representation.
///
/// Generic logical items without an emitted aggregate patch intentionally do
/// not implement this refinement, while remaining valid read-only children.
#[doc(hidden)]
pub trait SchemaPatchType: WireTypeSupport {
    type Patch<'source>: SchemaPatch<Self::Support>;
}

/// Binds a logical externally tagged payload declaration to its erased support.
///
/// Tagged payloads deliberately do not implement [`WireType`]: only a
/// containing record supplies their physical location and external tag. This
/// projection lets generated record code compose their opaque selected support
/// without exposing a union wire, a payload byte span, or tag storage.
#[doc(hidden)]
pub trait TaggedPayloadTypeSupport {
    type Tag: Copy + Eq;
    type Logical<'wire>;
    type Support: for<'wire> TaggedPayloadSupport<Tag = Self::Tag, Logical<'wire> = Self::Logical<'wire>>
        + 'static;
    type ZeroState: ZeroState;
    const LAYOUT: &'static LayoutDescriptor;
}

/// Patch-capable refinement for a generated externally tagged payload.
#[doc(hidden)]
pub trait TaggedPayloadPatchType: TaggedPayloadTypeSupport {
    type Patch<'source>: TaggedPayloadPatch<Self::Support>;
}

///
/// This remains a generated-code implementation detail so public aggregate
/// movement keeps the exact `copy_into` spelling on capabilities only.
#[doc(hidden)]
pub trait Materialize<Logical> {
    fn materialize(&self) -> Logical;
}

/// Maps generic runtime field handles back to one generated root's errors.
///
/// Generated owner adapters are zero-sized types. The mapping functions are
/// required so checked arithmetic failures never need an erased error object.
#[doc(hidden)]
pub trait OwnerAdapter {
    type AccessError: SchemaError;
    type MutationError: SchemaError + From<Self::AccessError>;

    fn access_layout(error: LayoutError) -> Self::AccessError;
    fn mutation_layout(error: LayoutError) -> Self::MutationError;
}

/// Safe generated support for a root or inline nested schema.
///
/// `validate` visits all declared fields in deterministic declaration order.
/// The default proof methods retain the exact validated span and brand it by
/// `Self`; generated constructors can therefore accept no layout-only input.
/// Patch commits remain deliberately write-only after complete preflight.
#[doc(hidden)]
pub trait SchemaSupport: Sized + InputAccess {
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;
    type Ref<'wire>;
    type Mut<'wire>;

    fn validate<'wire>(
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::AccessError>;

    fn prove<'wire>(
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<ProvedShared<'wire, Self, Self::Wire>, <Self::Owner as OwnerAdapter>::AccessError>
    {
        Self::validate(input)?;
        Ok(ProvedShared::new(input))
    }

    fn prove_mut<'wire>(
        input: ExclusiveInput<'wire, Self::Wire>,
    ) -> Result<ProvedExclusive<'wire, Self, Self::Wire>, <Self::Owner as OwnerAdapter>::AccessError>
    {
        Self::validate(input.shared())?;
        Ok(ProvedExclusive::new(input))
    }

    fn make_ref<'wire>(proof: ProvedShared<'wire, Self, Self::Wire>) -> Self::Ref<'wire>;

    fn make_mut<'wire>(proof: ProvedExclusive<'wire, Self, Self::Wire>) -> Self::Mut<'wire>;
    /// Derives this generated support's private write token only from an
    /// already-authorized exclusive input. Nested generated code uses this to
    /// forward a child token without exposing a free token constructor.
    fn input_token(input: &ExclusiveInput<'_, Self::Wire>) -> Self::Token;

    fn preflight_patch<'wire, P>(
        input: SharedInput<'wire, Self::Wire>,
        patch: &P,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>
    where
        P: SchemaPatch<Self>,
    {
        Self::validate(input).map_err(<Self::Owner as OwnerAdapter>::MutationError::from)?;
        patch.preflight(input)
    }

    fn commit_patch<'wire, P>(
        input: ExclusiveInput<'wire, Self::Wire>,
        patch: &P,
        token: Self::Token,
    ) where
        P: SchemaPatch<Self>,
    {
        patch.commit(input, token)
    }
}

/// Generated two-pass mutation from a borrowed logical record. This is used
/// by fixed-array adapters so `ArrayMut::set` and `copy_from` never need to
/// move, clone, or require `Copy` for nested logical elements.
#[doc(hidden)]
pub trait SchemaLogicalMutation<Logical>: SchemaSupport {
    fn preflight_logical<'wire>(
        input: SharedInput<'wire, Self::Wire>,
        value: &Logical,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit_logical<'wire>(
        input: ExclusiveInput<'wire, Self::Wire>,
        value: &Logical,
        token: Self::Token,
    );

    /// Preflights a complete source against an unproved destination without
    /// decoding the destination's current bytes.
    fn preflight_init_logical<'wire>(
        input: SharedInput<'wire, Self::Wire>,
        value: &Logical,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError> {
        Self::preflight_logical(input, value)
    }

    /// Initializes every active logical byte promised by a complete source.
    /// Callers perform all fallible work before this infallible commit.
    fn commit_init_logical<'wire>(
        input: ExclusiveInput<'wire, Self::Wire>,
        value: &Logical,
        token: Self::Token,
    ) {
        Self::commit_logical(input, value, token);
    }
}

/// Generated two-pass mutation from a borrowed logical tagged payload. This
/// keeps direct nested-record and fixed-array mutation independent of patch
/// ownership while preserving the containing record as the external-tag
/// coordinator.
#[doc(hidden)]
pub trait TaggedPayloadLogicalMutation<Logical>: TaggedPayloadSupport {
    fn logical_tag(value: &Logical) -> Self::Tag;

    fn preflight_logical<'wire>(
        current_tag: Self::Tag,
        payload: SharedInput<'wire, Self::Wire>,
        value: &Logical,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit_logical<'wire>(
        payload: ExclusiveInput<'wire, Self::Wire>,
        value: &Logical,
        token: Self::Token,
    );

    /// Preflights a complete selected payload source without reading the
    /// destination payload or an external tag.
    fn preflight_init_logical<'wire>(
        payload: SharedInput<'wire, Self::Wire>,
        value: &Logical,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError> {
        Self::preflight_logical(Self::logical_tag(value), payload, value)
    }

    /// Initializes a selected payload. The containing record commits its tag
    /// only after this payload write returns.
    fn commit_init_logical<'wire>(
        payload: ExclusiveInput<'wire, Self::Wire>,
        value: &Logical,
        token: Self::Token,
    ) {
        Self::commit_logical(payload, value, token);
    }
}

/// Materializes a source-lifetime-rebound logical schema only from exact proof
/// branded by its lifetime-erased generated wire support.
#[doc(hidden)]
pub trait LogicalSchema<'wire>: WireTypeSupport + Sized {
    fn materialize(
        proof: ProvedShared<'wire, <Self as WireTypeSupport>::Support, <Self as WireType>::Wire>,
    ) -> Self;
}
/// Generated patch behavior for one schema support implementation.
///
/// A separate patch trait permits a logical declaration with any number of
/// source lifetimes to implement the contract without collapsing them into one
/// artificial lifetime parameter.
#[doc(hidden)]
pub trait SchemaPatch<S: SchemaSupport> {
    /// Whether this patch supplies a complete logical replacement. This is
    /// used to prove tagged-variant switches and absent Optional promotion
    /// before any destination byte moves.
    fn is_complete(&self) -> bool;

    fn preflight<'wire>(
        &self,
        input: SharedInput<'wire, S::Wire>,
    ) -> Result<(), <S::Owner as OwnerAdapter>::MutationError>;

    fn commit<'wire>(&self, input: ExclusiveInput<'wire, S::Wire>, token: S::Token);

    /// Preflights a complete patch against an unproved destination without
    /// decoding destination bytes.
    fn preflight_init<'wire>(
        &self,
        input: SharedInput<'wire, S::Wire>,
    ) -> Result<(), <S::Owner as OwnerAdapter>::MutationError> {
        self.preflight(input)
    }

    /// Initializes every active byte promised by a complete patch after all
    /// preflight succeeds.
    fn commit_init<'wire>(&self, input: ExclusiveInput<'wire, S::Wire>, token: S::Token) {
        self.commit(input, token);
    }
}

/// Generated support for a closed scalar enum representation.
#[doc(hidden)]
pub trait ScalarEnumSupport: SchemaSupport {
    type Raw: Copy + Eq;
    type Value: Copy + Eq;

    fn raw(input: SharedInput<'_, Self::Wire>) -> Self::Raw;
    fn from_raw(raw: Self::Raw) -> Option<Self::Value>;
    fn to_raw(value: Self::Value) -> Self::Raw;

    /// Stores a declared scalar-enum value into an exact selected field after
    /// the enclosing patch has completed all fallible preflight work.
    fn commit(input: ExclusiveInput<'_, Self::Wire>, value: Self::Value, token: Self::Token);
}
/// Generated field-local adapter for a scalar, Boolean, or scalar enum.
///
/// `preflight` validates only the new logical source value. `commit` receives
/// an exact checked field input and must be an infallible selected-range write.
/// The generated owner adapter keeps every error concrete to its root schema.
#[doc(hidden)]
pub trait ScalarMutationAdapter: InputAccess {
    type Wire: FromBytes + KnownLayout + Immutable + Copy + 'static;
    type Owner: OwnerAdapter;
    type Logical: Copy;

    fn read(
        input: SharedInput<'_, Self::Wire>,
    ) -> Result<Self::Logical, <Self::Owner as OwnerAdapter>::AccessError>;

    fn preflight(value: Self::Logical) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit(input: ExclusiveInput<'_, Self::Wire>, value: Self::Logical, token: Self::Token);
}

/// Generated field-local adapter for one bounded string representation.
///
/// The logical string is deliberately unsized (`str`, `CStr`, `U16Str`, or
/// `U16CStr`). The preflight receives the current checked field so it can prove
/// capacity and prefix representability before `commit` touches any byte.
#[doc(hidden)]
pub trait StringMutationAdapter: InputAccess {
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;
    type Logical: ?Sized;

    fn read<'wire>(
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<&'wire Self::Logical, <Self::Owner as OwnerAdapter>::AccessError>;

    fn preflight(
        input: SharedInput<'_, Self::Wire>,
        value: &Self::Logical,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit(input: ExclusiveInput<'_, Self::Wire>, value: &Self::Logical, token: Self::Token);
}

/// Generated field-local adapter for one fixed byte field.
#[doc(hidden)]
pub trait FixedBytesMutationAdapter: InputAccess {
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;

    fn read(
        input: SharedInput<'_, Self::Wire>,
    ) -> Result<&[u8], <Self::Owner as OwnerAdapter>::AccessError>;

    fn preflight(value: &[u8]) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit(input: ExclusiveInput<'_, Self::Wire>, value: &[u8], token: Self::Token);
}

/// Generated field-local adapter for a zero-sentinel optional field.
///
/// `StorageWire` is the complete declared field span, including an alignment
/// wrapper and its padding. `ValueWire` begins at `VALUE_OFFSET` within that
/// span and is proved only after the storage is known nonzero.
#[doc(hidden)]
pub trait OptionFieldAdapter: Sized + InputAccess {
    type StorageWire: FromBytes + KnownLayout + Immutable + 'static;
    type ValueWire: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;
    type Read<'wire>;
    type Value<'source>;
    type Mut<'wire>;

    const VALUE_OFFSET: usize;

    fn validate_present(
        input: SharedInput<'_, Self::ValueWire>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::AccessError>;

    fn read_present<'wire>(
        input: SharedInput<'wire, Self::ValueWire>,
    ) -> Result<Self::Read<'wire>, <Self::Owner as OwnerAdapter>::AccessError>;

    fn make_present_mut<'wire>(
        input: ExclusiveInput<'wire, Self::ValueWire>,
        token: Self::Token,
    ) -> Result<Self::Mut<'wire>, <Self::Owner as OwnerAdapter>::AccessError>;

    /// Validates a complete source without decoding the current destination.
    fn preflight_init<'wire, 'source>(
        input: SharedInput<'wire, Self::ValueWire>,
        value: &Self::Value<'source>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    /// Writes a fully preflighted source into the selected value span.
    fn commit_init<'wire, 'source>(
        input: ExclusiveInput<'wire, Self::ValueWire>,
        value: &Self::Value<'source>,
        token: Self::Token,
    );

    /// Clears exactly the declared storage span, including any field-local
    /// alignment padding, while retaining the generated private token gate.
    #[inline]
    fn clear(mut input: ExclusiveInput<'_, Self::StorageWire>, token: Self::Token) {
        input.clear_all::<Self>(token);
    }
}

/// Generated support for an externally tagged payload declaration.
///
/// The payload support owns selected-payload proof and logical materialization,
/// but never stores or writes the sibling external tag. The containing record
/// remains the only coordinator allowed to commit that tag after payload bytes.
#[doc(hidden)]
/// Generated support for an externally tagged payload declaration.
///
/// A selected-payload capability can only be built from a selection token that
/// binds one decoded tag to the exact payload span that was validated for it.
/// Patch methods are payload-only write contracts and cannot mint a capability.
#[doc(hidden)]
pub trait TaggedPayloadSupport: Sized + InputAccess {
    type Tag: Copy + Eq;
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;
    type Logical<'wire>;
    type Ref<'wire>;
    type Mut<'wire>;

    fn validate_selected<'wire>(
        tag: Self::Tag,
        payload: SharedInput<'wire, Self::Wire>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::AccessError>;
    /// Derives this payload support's token from an exclusive range selected
    /// under an already-authorized containing root.
    fn input_token(input: &ExclusiveInput<'_, Self::Wire>) -> Self::Token;

    fn make_ref<'wire>(selection: TaggedRefSelection<'wire, Self>) -> Self::Ref<'wire>;

    fn make_mut<'wire>(selection: TaggedMutSelection<'wire, Self>) -> Self::Mut<'wire>;

    fn materialize_selected<'wire>(
        selection: TaggedRefSelection<'wire, Self>,
    ) -> Self::Logical<'wire>;

    fn patch_tag<P>(patch: &P) -> Self::Tag
    where
        P: TaggedPayloadPatch<Self>;

    fn patch_is_complete<P>(patch: &P) -> bool
    where
        P: TaggedPayloadPatch<Self>;

    fn preflight_patch<'wire, P>(
        current_tag: Self::Tag,
        payload: SharedInput<'wire, Self::Wire>,
        patch: &P,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>
    where
        P: TaggedPayloadPatch<Self>;

    /// Preflights a complete selected payload patch without inspecting the
    /// inactive destination representation.
    fn preflight_patch_init<'wire, P>(
        payload: SharedInput<'wire, Self::Wire>,
        patch: &P,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>
    where
        P: TaggedPayloadPatch<Self>;

    /// Commits only selected payload bytes. The containing record writes its
    /// external tag last after this returns.
    fn commit_patch<'wire, P>(
        payload: ExclusiveInput<'wire, Self::Wire>,
        patch: &P,
        token: Self::Token,
    ) where
        P: TaggedPayloadPatch<Self>;

    /// Commits a complete selected payload without decoding its inactive
    /// destination representation. Generated external-tag switches invoke
    /// this before storing their new tag.
    fn commit_patch_init<'wire, P>(
        payload: ExclusiveInput<'wire, Self::Wire>,
        patch: &P,
        token: Self::Token,
    ) where
        P: TaggedPayloadPatch<Self>,
    {
        patch.commit_init(payload, token);
    }
}

/// Generated patch behavior for one tagged payload declaration.
#[doc(hidden)]
pub trait TaggedPayloadPatch<S: TaggedPayloadSupport> {
    fn tag(&self) -> S::Tag;
    fn is_complete(&self) -> bool;

    fn preflight<'wire>(
        &self,
        current_tag: S::Tag,
        payload: SharedInput<'wire, S::Wire>,
    ) -> Result<(), <S::Owner as OwnerAdapter>::MutationError>;

    /// Commits payload bytes only; tag storage is intentionally unavailable.
    fn commit<'wire>(&self, payload: ExclusiveInput<'wire, S::Wire>, token: S::Token);

    /// Preflights a complete selected payload patch without decoding the
    /// destination payload or current external tag.
    fn preflight_init<'wire>(
        &self,
        payload: SharedInput<'wire, S::Wire>,
    ) -> Result<(), <S::Owner as OwnerAdapter>::MutationError> {
        self.preflight(self.tag(), payload)
    }

    /// Initializes a selected payload after all root preflight succeeds.
    fn commit_init<'wire>(&self, payload: ExclusiveInput<'wire, S::Wire>, token: S::Token) {
        self.commit(payload, token);
    }
}

/// Generated support for one fixed-array element under one owning schema.
///
/// All source checks occur in `preflight`; `commit` receives an exact selected
/// element range after the complete array preflight has succeeded and is
/// therefore infallible. `index_error` and `length_error` preserve the root's
/// concrete field/index error shape without erased runtime errors.
#[doc(hidden)]
pub trait ArrayElementAdapter: InputAccess {
    type Wire: FromBytes + KnownLayout + Immutable + 'static;
    type ArrayWire<const N: usize>: FromBytes + KnownLayout + Immutable + 'static;
    type Owner: OwnerAdapter;

    /// The zero-copy logical observation returned by reads. For nested schema
    /// elements this is the child `Ref`, not the logical source record.
    type Read<'wire>;

    /// The declared logical source accepted by `set` and `copy_from`. This is
    /// deliberately distinct from [`Self::Read`], because a nested `Ref` is a
    /// capability and must never become an implicit mutation source.
    type Value<'source>;

    type Mut<'wire>;

    /// Physical distance between adjacent element starts, including ABI stride.
    const STRIDE: usize;

    fn prove<'wire>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::AccessError>;

    fn read<'wire>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<Self::Read<'wire>, <Self::Owner as OwnerAdapter>::AccessError>;

    fn make_mut<'wire>(
        index: usize,
        input: ExclusiveInput<'wire, Self::Wire>,
        token: Self::Token,
    ) -> Result<Self::Mut<'wire>, <Self::Owner as OwnerAdapter>::AccessError>;
    fn preflight<'wire, 'value>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError>;

    fn commit<'wire, 'value>(
        index: usize,
        input: ExclusiveInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
        token: Self::Token,
    );

    /// Preflights a complete element source without reading destination bytes.
    /// Generated adapters override this for absent Optional promotion; the
    /// compatibility default is only for pre-existing handwritten adapters
    /// that cannot be selected for that generated operation.
    fn preflight_init<'wire, 'value>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError> {
        Self::preflight(index, input, value)
    }

    /// Commits one fully preflighted element initialization.
    fn commit_init<'wire, 'value>(
        index: usize,
        input: ExclusiveInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
        token: Self::Token,
    ) {
        Self::commit(index, input, value, token);
    }

    fn index_error(index: usize, len: usize) -> <Self::Owner as OwnerAdapter>::MutationError;

    fn length_error(actual: usize, expected: usize)
    -> <Self::Owner as OwnerAdapter>::MutationError;
}

/// Layout-only inputs and runtime-private exact-input proof wrappers used by
/// generated composition. `SharedInput` and `ExclusiveInput` alone cannot
/// construct logical capabilities.
#[doc(hidden)]
pub use crate::access::{ExclusiveInput, ProvedExclusive, ProvedShared, SharedInput};

/// Checked fixed-array offset helpers.
#[doc(hidden)]
pub use crate::array::{checked_element_offset, checked_element_range};

/// Allocation-free structured-error formatter for generated error displays.
#[doc(hidden)]
pub use crate::error::__fmt_schema_error;

/// Checked range and bounded byte-write helpers.
#[doc(hidden)]
pub use crate::mutation::{checked_range, copy_bytes_at, copy_exact, subrange_mut};

/// Bounded string proof, source-preflight, and infallible commit helpers.
#[doc(hidden)]
pub use crate::strings::{
    StringMutationError, StringProofError, commit_c_str, commit_str, commit_u16_c_str,
    commit_u16_str, invalid_utf8_source, preflight_c_str, preflight_length_prefixed, preflight_str,
    preflight_u16_c_str, preflight_u16_str, prove_c_str, prove_str, prove_u16_c_str,
    prove_u16_c_str_bytes, prove_u16_str, prove_u16_str_bytes, set_c_str, set_str, set_u16_c_str,
    set_u16_str,
};

/// Logical wide-string types used by generated bounded-string adapters.
#[doc(hidden)]
pub use widestring::{U16CStr, U16Str};

/// Checked external-tag payload selection and payload-before-tag commit helpers.
#[doc(hidden)]
pub use crate::tagged::{
    TaggedMutSelection, TaggedRefSelection, checked_payload_range, checked_tagged_ranges,
    commit_payload_before_tag, commit_payload_before_tag_with,
};

/// All explicit wire forms selected by generated layout support.
#[doc(hidden)]
pub use crate::wire::{
    Align1, Align2, Align4, Align8, Align16, Align32, Align64, Align128, Align256, Align512,
    Align1024, Align2048, Align4096, Align8192, Align16384, Align32768, Align65536, Align131072,
    Align262144, Align524288, Align1048576, Align2097152, Align4194304, Align8388608,
    Align16777216, Align33554432, Align67108864, Align134217728, Align268435456, Align536870912,
    AlignedWire, BigF32, BigF64, BigI16, BigI32, BigI64, BigU16, BigU32, BigU64, BoolWire,
    CStrWire, I8, LengthWire, LittleF32, LittleF64, LittleI16, LittleI32, LittleI64, LittleU16,
    LittleU32, LittleU64, NativeF32, NativeF64, NativeI16, NativeI32, NativeI64, NativeU16,
    NativeU32, NativeU64, ScalarWire, StrWire, U8, U16CStrWire, U16StrWire,
};
