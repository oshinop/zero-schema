# Changelog

All notable public changes are documented here. `zero-schema` follows semantic
versioning for its Rust API; applications version their own byte protocols
independently.

## Unreleased

### Added

- **Heap receiving storage (`alloc`).** `SchemaBuffer::new_boxed() -> Box<Self>` and
  `make_boxed_schema_buffer!(Root)` build the zero-filled receiving buffer directly on
  the heap, with no stack temporary of the root's size, no `unsafe`, and the same
  alignment as the inline buffer. The macro applies the same fully-concrete-root
  compile-time checks as `make_schema_buffer!`. Semantics match `new()`: initialized
  bytes, no implied schema validity. `SchemaBuffer` now derives `zerocopy::FromZeros`.
- **Prefix access.** Every root (record or scalar enum) generates
  `access_prefix(&[u8]) -> Result<(RootRef<'_>, &[u8]), RootAccessError>` and
  `access_mut_prefix(&mut [u8]) -> Result<(RootMut<'_>, &mut [u8]), RootAccessError>`.
  They require at least `SCHEMA_SIZE` bytes, prove exactly the leading `SCHEMA_SIZE` bytes
  with the alignment rule and eager proof of `access`, and return the unexamined,
  disjoint remainder. A shorter span fails with the existing layout error kind
  (`LayoutError::InsufficientBytes { required: SCHEMA_SIZE, actual }`) before any
  alignment test. `access` and `access_mut` remain exact-length.
- **String capacity metadata.** `StringMut::capacity(&self) -> usize` returns the field's
  declared `#[zero(capacity = N)]` in storage units: bytes for `str` and `CStr`, `u16`
  units for `U16Str` and `U16CStr`. C-string forms count their terminator; length-prefixed
  forms exclude the prefix. It is read-only metadata: no setter truncates, and
  `StringDescriptor::capacity()` in `LAYOUT` reports the same value and is now documented
  with these units. It is available on every `StringMut`, including the per-element
  handles of string arrays.
- Fixed arrays of bounded strings: `[&str; N]`, `[&CStr; N]`, `[&U16Str; N]`, and
  `[&U16CStr; N]`. The field's `capacity` (and `len_type`/`endian` where the string
  form has a prefix) applies to every element; each element is that string's own
  wire and the stride is its compiler size, matching C/C++ `wchar_t names[N][CAP]`.
  `ArrayRef` yields the borrowed string per element (zero-copy), `ArrayMut`
  yields a per-element `StringMut` that preserves unused capacity, and
  `set`/`copy_from`/patches take borrowed strings with exact full-length,
  preflight-first transfer. The logical value is the declared `[&str; N]` array.
- Fixed arrays of fixed arrays, `[[T; N]; M]` to any depth, with the row-major C++
  multidimensional layout. A row is an `ArrayRef`/`ArrayMut` view; `set` and
  `copy_from` move whole `[T; N]` rows, and the record-level `copy_into` and
  patches use the declared `[[T; N]; M]` value. Failing elements report every
  enclosing index (`field[i][j]`), for access and for mutation.
- Layout metadata: `ArrayElementKind::String(StringDescriptor)` and
  `ArrayElementKind::Array(&ArrayDescriptor)` (the enum is `#[non_exhaustive]`).
- **Compile-time zero validity.** Every record (root or nested) and scalar enum has
  `pub const ZERO_IS_VALID: bool`, the exact answer to whether an all-zero span is
  type-valid, that is, whether `access` and `access_mut` accept it. Primitives, `bool`,
  fixed bytes, every bounded string form (including string arrays), and `Option<T>` are
  always zero-valid; a scalar enum is zero-valid iff a declared discriminant is `0`;
  fixed arrays (including nested arrays) and nested schemas take their element's or
  schema's value; a tagged-union field needs the external tag enum to have a
  discriminant-`0` variant whose payload is zero-valid. It is a true `const`, usable in
  `const { assert!(Root::ZERO_IS_VALID) }` for a concrete root; a generic root's value
  depends on its type arguments, so name it with every parameter supplied.
- **Checked zero-start receiving storage.** `make_zeroed_schema_buffer!(Root)` and,
  under `alloc`, `make_zeroed_boxed_schema_buffer!(Root)` apply the fully-concrete-root
  checks of `make_schema_buffer!`/`make_boxed_schema_buffer!` and additionally fail to
  compile, with a message naming `ZERO_IS_VALID`, when the root's all-zero span is not
  valid (for example an enum field with no `= 0` variant; `Option<Enum>` fixes it). The
  result is documented as type-valid until its bytes are changed, so the first `access`
  or `access_mut` succeeds. They return only `SchemaBuffer` storage; `access` and
  `access_mut` remain the sole, eagerly proved route to a capability.
- **`clear()` for zero-valid records and every string form.** Every generated record
  mutable capability (`<Record>Mut`, whether a root, a nested record or an array element)
  gets `pub fn clear(&mut self)`. It zero-fills exactly the record's complete declared
  storage, the `SCHEMA_SIZE` bytes its access proof covers including its internal
  padding, and nothing outside that span; a nested record or array element clears only
  its own span and leaves the parent's other fields and padding alone. It reuses the
  private-token-gated clearing path of `OptionMut::set(None)`. The method exists only
  where an all-zero span is certainly a valid value of the record, so the result stays
  valid: a call on any other record fails to type-check at the call site (also under
  `cargo check`), with a `#[diagnostic::on_unimplemented]` message naming
  `ZERO_IS_VALID` and the `Option<_>` fix (give an enum a `= 0` variant, or make the
  field `Option<Enum>`). There is no runtime check and no panic path.
  `StringMut::clear(&mut self)` zero-fills a string field's complete storage, including
  any length prefix and all capacity, so the field reads back as the empty string; it is
  available for all four string forms (`&str`, `&CStr`, `&U16Str`, `&U16CStr`), in both
  prefix forms, and on the per-element handle of a string array. `set` is unchanged and
  still keeps unused capacity.

  The `clear()` gate is stricter than `ZERO_IS_VALID` for one construct: a record that
  holds an externally tagged payload can be cleared only when some variant claims the
  zero tag and every variant payload is zero-valid. `ZERO_IS_VALID` looks only at the
  variant the zero tag selects, so a record can have `ZERO_IS_VALID == true` and still
  not offer `clear()`; the gate cannot tell at the type level which variant a zero tag
  picks.

### Changed

- **Compatible dependency ranges.** `zerocopy` (`0.8.54`) and `widestring` (`1.2.1`) are
  now caret requirements in every manifest, README snippet, and the package-pair
  verifier, not exact pins. A consumer may use any later `0.8.x` `zerocopy` (for example
  0.8.61); Cargo resolves the runtime and the consumer's direct `zerocopy` dependency to
  one release, and generated code relies only on the `FromBytes`, `KnownLayout`, and
  `Immutable` derives from that dependency. The `zero-schema` to `zero-schema-macros`
  requirement stays the exact lockstep pair `=0.1.0`.
- A symbolic array length is now asserted nonzero at every nesting level, not only
  the outermost. `[T; 0]` stays forbidden everywhere; `Option` of a string or nested
  array stays rejected.
- The UI fixture `14_unsupported_array_element` now uses a still-unsupported element
  (`&[u8; N]`) because `[&str; N]` with `capacity` is supported.

### Compatibility notes

- `access_prefix` and `access_mut_prefix` are reserved generated names: a record field or
  scalar-enum variant with either name is rejected with the generated-name collision
  diagnostic.
- `ZERO_IS_VALID` is a reserved generated name: a record field or scalar-enum variant
  with that name is rejected with the generated-name collision diagnostic.
- `clear` is a reserved generated name: a record field with that name is rejected with
  the generated-name collision diagnostic, because `<Record>Mut` now has a `clear`
  method. Rename the field, for example to `clear_flag`.
- The doc-hidden `StringMutationAdapter` macro contract gained a required associated
  constant (`CAPACITY`) and `SharedInput`/`ExclusiveInput` gained `from_prefix`, so a
  `zero-schema-macros` expansion must be paired with the matching runtime — the exact
  `=0.1.0` pair already requires this.
- The doc-hidden `WireTypeSupport` and `TaggedPayloadTypeSupport` traits gained a
  required associated constant (`ZERO_IS_VALID`) that composes nested and tagged
  validity; the exact `=0.1.0` macro/runtime pair already covers this. The sealed
  `ZeroState` that gates `Option<T>` eligibility is unchanged and stays conservative for
  tagged payloads (a payload is possibly zero-valid when any variant is), whereas
  `ZERO_IS_VALID` uses the zero tag's variant.
- The doc-hidden `WireTypeSupport` and `TaggedPayloadTypeSupport` traits gained a
  required associated type (`ClearState`), a second sealed `ZeroState` that
  under-approximates `ZERO_IS_VALID` and gates `clear()`: it equals `ZeroState` except
  for a tagged payload, which is zero-valid there only when the zero tag is claimed and
  every variant payload is. The `Option<T>` rules that read `ZeroState` are untouched.
  The doc-hidden `Gate`, `ZeroClear`, `ClearableState`, `ZeroFlag`, and `ZeroFlagState`
  carry the call-site bound; again the exact `=0.1.0` pair covers the new associated
  type.

## 0.1.0

Initial public release of `zero-schema` and `zero-schema-macros`.

### Public contract

- `#[zero]` declares named record schemas, closed scalar-enum schemas, and logical
  tagged payload declarations. `zero-schema` unconditionally depends on and re-exports
  the attribute from `zero-schema-macros`. The default feature set is only `std`;
  `alloc` is independently opt-in; the host-only macro enables neither `alloc` nor
  `std` and adds no target-binary behavior.
- Schemas observe producer-owned, already initialized bytes with exact eager
  `access` and `access_mut` constructors. Successful calls return compact logical
  capabilities only after representation safety and declared bounds are proven.
- Root records and scalar enums expose `SCHEMA_SIZE`, `SCHEMA_ALIGN`,
  `SCHEMA_STRIDE`, and diagnostic `LAYOUT` metadata. `schema_buffer!` names the
  aligned initialized receiving-storage type for a fully concrete root, while
  `make_schema_buffer!` constructs a value. Initial bytes carry no promised schema
  interpretation.
- Shared capabilities expose field-named reads and `copy_into`. Exclusive
  capabilities expose short field-local mutable reborrows, constrained field
  updates, and transactional `copy_from` patches. `ArrayRef`/`ArrayRefIter` and
  `ArrayMut` support indexed operations and exact-length transfer without allocation.
- A supported field-level `Option<T>` has no presence byte: `None` iff every byte
  of its complete declared StorageWire span is zero. Its all-zero-invalid inner is
  eagerly proved when any span byte is nonzero. `OptionMut` exposes `get`,
  `get_mut`, and `set(None | Some(_))`; generated optional patches are
  `Option<Option<P>>` for retain, clear, and present-update states. Primitive,
  Boolean, string, fixed-byte, nested-Option, Option-element, and tagged-payload
  inners are rejected.
- A tagged payload is permitted only as a record field with a unique external scalar
  `tag_field`. The payload capability coordinates selected reads, mutation, and
  complete variant changes; an external tag has no independent mutable surface.
- Access and mutation errors retain allocation-free structured paths. Core proof,
  reads, mutation, arrays, option spans, union selection, logical materialization,
  patches, metadata, and error traversal work under `#![no_std]` without allocation.
  `alloc` adds the owned error-path convenience.
- Generated wire support resolves the consuming crate's direct `zerocopy`
  dependency. A crate using `#[zero]` therefore declares both `zero-schema` and
  `zerocopy` directly.

### Compatibility and interoperability boundaries

- A tagged payload is not a root and has no independent layout constants, receiving
  storage, or access constructor.
- The producer is responsible for initialized storage and ABI agreement. Inline C
  values have no universal `NULL`; optional absence is an explicit all-zero complete
  field representation, produced by initializing every byte and zeroing the field
  (for example, `memset(&record.field, 0, sizeof record.field)`). Fixed-width
  integer storage and target/compiler/flags/endian assertions remain required.
- The repository's C++17 fixtures check selected ABI profiles but do not generate
  consumer headers or negotiate schema versions. Scalar enums and tagged payloads are
  closed. Adding a discriminant or payload variant is a breaking change for older
  readers. Native-wide string units require a matching target ABI and native endianness.
