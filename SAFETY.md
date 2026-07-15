# Safety

## Scope and responsibility boundary

`zero-schema` is a safe observer and constrained mutator of an **already initialized**
byte representation. The byte producer—such as a shared-memory peer, C or C++ code,
a device, or a reviewed fixture—owns construction of that representation. The Rust
API has no safe operation that turns arbitrary bytes into a promised-valid schema.

`access` and `access_mut` accept ordinary Rust slices, so safe Rust already prevents
reading uninitialized memory. At an FFI or shared-memory boundary, the caller is
responsible for establishing the corresponding conditions before constructing those
slices. The runtime checks exact length, address alignment, and schema type validity;
it cannot repair a producer that violates Rust's initialization, lifetime, or
data-race rules.

### Producer/consumer checklist

**Producer and storage integration** must:

- initialize every byte in the exact root span before Rust forms a slice, including
  parent padding, bounded-string unused capacity, and inactive union payload storage;
- for a successful root access, provide exactly `SCHEMA_SIZE` bytes whose first byte
  meets `SCHEMA_ALIGN`, keep the allocation live, and prevent external mutation or
  reuse for every derived capability. Shared capabilities require a stable span;
  constrained writes occur only through an exclusive mutable capability; and
- write only declared scalar/string representations, and keep each external tag
  coupled to its selected payload. `Option<T>::None` is all zeroes in its complete
  optional field span, not a convention for parent padding.

**Consumers** must:

- obtain a capability with `access` or `access_mut` before logical reads or writes;
  a failed proof grants no view of the representation;
- treat `SchemaBuffer` as aligned, initialized Rust receiving storage only. Its initial
  zero-fill does not initialize a schema; producer bytes still require `access`; and
- use only capability operations. Mutable field, nested, selected-payload, and option
  handles are short reborrows; if any fallible mutation returns an error, every byte
  of the destination root span is unchanged.

Executable application paths are cross-linked here rather than reproduced as a
tutorial: [records](examples/records.rs), [strings](examples/strings.rs),
[zero-sentinel options](examples/optional.rs), [externally tagged payloads](examples/tagged.rs),
[access diagnostics](examples/access_errors.rs),
[generic receiving storage](examples/generic_receiving_buffer.rs), and
[freestanding `no_std` access](examples/no_std_wasm.rs). The
[example map](examples/README.md) provides their focused commands.

This document covers Rust memory safety and the representation guarantees made by the
library. It does not certify an application's protocol semantics, freshness,
authorization, cross-process synchronization, or ABI compatibility with an
independently maintained producer.

## Access proof

A generated root constructor applies this sequence:

1. It checks that the supplied span length is exactly `SCHEMA_SIZE`; size failure
   takes precedence over an alignment failure.
2. It checks the address against `SCHEMA_ALIGN` before forming any typed wire view.
3. It keeps the original bounded byte input private. Every generated child selection
   uses checked offset addition, exact subrange bounds, and required child alignment.
4. It eagerly walks the entire logical declaration in source order. It checks Boolean
   representations, closed scalar-enum values, active length bounds, UTF-8, required
   narrow/wide terminators, array elements in increasing index order, nested records,
   and the one selected payload associated with each external tag.
5. It returns a capability only if every required check succeeded. `access_mut` runs
   the same proof through a shared reborrow before it creates an exclusive capability.

The proof deliberately ignores ordinary compiler padding outside a zero-sentinel
`Option` StorageWire, unused bounded-string capacity, and inactive union payload
storage. Those ordinary ignored bytes have no logical interpretation and are never
required to be zero. Optional StorageWire padding is the explicit exception below:
it is presence-significant. A capability is therefore a type-valid snapshot of the
stable bytes it borrows, not a cache of decoded values or an assertion about ignored
storage.

### Zero-sentinel `Option` invariants

An accepted `Option<T>` has no presence byte. `None` means every byte of its
complete `FieldDescriptor::offset()..offset() + size()` storage span is zero. The
scan and clear include field-alignment wrapper and inner padding; parent inter-field
and root trailing padding are excluded. Any nonzero byte means `Some`, so the normal
eager proof of `T` must succeed.

Eligibility is restricted to all-zero-invalid scalar-enum and schema paths, plus
nonempty arrays of eligible elements: no valid `Some` may collide with the all-zero
representation. This is not a general-purpose `Option` encoding. The normative
complete-span representation and private-adapter contract are in the
[design RFC's Option section](docs/zero-schema-design-rfc.md#74-zero-sentinel-option-representation)
and [memory-safety argument](docs/zero-schema-design-rfc.md#14-memory-safety-argument),
not public extension points.

No public capability exposes a wire reference, raw pointer, raw byte slice, or union
member. Strings borrow only after their active representation is proven; scalar enums
are returned only after their discriminant is valid; and an external union selects a
payload only after its sibling scalar tag is valid.

## Borrowing and zero-copy capabilities

A shared root capability is `Copy + Clone`. Its scalar getters return values and its
borrowed string/fixed-byte getters are constrained to the source lifetime. A mutable
root capability is exclusive and non-`Copy`.

A mutable read borrows through `&self` for only the current shared reborrow. A mutable
field method produces a short, field-local exclusive capability. Nested and selected
payload mutation use the same rule. This prevents safe callers from holding a mutable
aggregate view, raw storage access, or an independently mutable external tag while
also observing another part of the schema. It also lets the implementation revisit
private bounded ranges rather than manufacture overlapping mutable references.

Array capabilities maintain O(1) state. Their element lookups, iteration, and logical
materialization use bounded indexed selections; no decoded aggregate or heap proxy
collection is retained by the capability.

`OptionMut<'view, T, _>` is equally O(1) and field-local. `get()` and `get_mut()`
rescan the live complete field span rather than cache presence. A child from
`get_mut()` holds the short exclusive reborrow; `set(None)` cannot overlap it and
zeroes only the complete optional field span. `set(Some(value))` preflights the
inner initialization before it writes, so a source error leaves that field unchanged.

## Constrained mutation and atomicity

Mutation starts only after `access_mut` has established type validity. Field handles
validate all fallible source conditions before their first write. This includes string
capacity and length representability, fixed-byte and array length, element conversion,
nested inputs, and optional inner initialization. Any fallible mutation error leaves
every byte of the destination root span byte-for-byte unchanged.

`ArrayMut::copy_from` validates the complete source slice and each element before it
commits any element. Generated record and tagged-payload patches use the same two-pass
rule: recursively validate every present member and external-tag relationship, then
perform bounded infallible writes. An optional patch entry is `Option<Option<P>>`:
outer `None` retains, `Some(None)` clears, and `Some(Some(P))` updates a present field
or promotes an absent field only when `P` is complete. An absent incomplete promotion
reports `IncompleteOptionalInitialization` before its inner preflight; any patch error
leaves every destination byte unchanged.

An external union has a single coordinator: its payload capability. There is no
independent tag-field mutation method. A patch that switches variants must be
recursively complete; the commit writes the new selected payload first and the
external scalar tag last. A tag-only or mismatched tag/payload patch fails before a
write. Successful constrained mutation retains type validity, so a fresh `access`
observes the new logical state.

A data-carrying tagged enum is not root-accessible or root-bufferable: only its
containing record supplies the external tag location. The record keeps that tag and
payload coupled throughout proof and mutation.

## Layout, bounded work, and allocation

Generated constants assert root and child size, alignment, stride, offsets, array
stride, and nonzero layout constraints. Offset arithmetic is checked before use. The
work of proof, field reads, mutation, array traversal, external-union selection,
optional complete-span scans/clears, logical materialization, patches, layout
inspection, and structured-error traversal is bounded by the fixed representation and
declared capacities; these core operations allocate nothing.

`LAYOUT` is diagnostic metadata only. It describes the compiler-derived ABI shape,
including padding ranges and `FieldDescriptor::is_optional()`, but it never selects
memory or relaxes access checks. The optional protocol derives from that field's exact
descriptor span, not from generic parent padding metadata. Allocation may occur only
in caller code or optional convenience APIs such as an owned formatted error path
enabled by `alloc`.

## Normative design and evidence

The [normative design RFC](docs/zero-schema-design-rfc.md) defines the complete
representation and generated-proof model; its [memory-safety argument](docs/zero-schema-design-rfc.md#14-memory-safety-argument)
and [stable-snapshot boundary](docs/zero-schema-design-rfc.md#15-stable-snapshots-and-concurrency)
are authoritative for private implementation details. This page states the
application responsibility boundary and public invariants without making those private
forms extension points.

The [C++ conformance and reviewed-fixture evidence](TESTING.md#c-conformance-and-reviewed-fixtures)
checks selected target profiles through C++ producer/observer fixtures and Rust
capabilities. It is evidence for those profiles, not a generated C++ header, a
producer implementation, or a compatibility negotiation protocol.

## Interoperability

Memory safety is not a declaration that independently compiled layouts are compatible.
A C or C++ producer must use the same target ABI/data model, compiler flags, and endian
profile; fixed-width integer storage; and default non-packed layout. Each integration
must retain target-side layout assertions—`static_assert` in C++ or `_Static_assert` in
C—for root and payload size/alignment, relevant field `offsetof`, array stride,
optional-field size, external-tag offset, and scalar representations. C/C++ `bool`,
enum storage, `wchar_t`, bitfields, pointer punning, packing directives, and
ABI-changing compiler flags are outside the contract.

Inline C values have no universal `NULL`: optional absence is the explicit all-zero
complete optional field representation, produced only after every transported byte is
initialized (for example, `memset(&record.field, 0, sizeof record.field)`). Parent
padding is still initialized transport storage, but it is not part of the optional
presence scan. The [RFC interoperability requirements](docs/zero-schema-design-rfc.md#17-c-and-c-interoperability)
and linked conformance evidence above define the supported boundary.

Report security concerns through the project's private security-reporting channel on
the hosting platform when one is available.
