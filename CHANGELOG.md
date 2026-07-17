# Changelog

All notable public changes are documented here. `zero-schema` follows semantic
versioning for its Rust API; applications version their own byte protocols
independently.

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
