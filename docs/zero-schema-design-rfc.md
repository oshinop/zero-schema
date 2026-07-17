# `zero-schema`: checked zero-copy wire capabilities

**Status:** design RFC  
**Document version:** 0.12  
**Date:** 2026-07-15  
**Reference implementation baseline:** `zerocopy` 0.8.54 and `widestring` 1.2.1  
**Supersedes for review:** the materializing v0.2 proposal, copy-interop v0.9 draft, and aggregate-copy v0.10 draft

> **Scope.** `zero-schema` gives Rust code checked, zero-copy access to an
> existing fixed-layout wire and constrained mutation of an already type-valid
> wire. It can explicitly materialize logical aggregates out of that wire and
> transfer logical patches into that wire without allocation. No public operation
> initializes arbitrary bytes, exposes unrestricted mutable wire storage, or
> performs application or domain validation.

---

## Table of contents

1. [Abstract](#1-abstract)
   - [Runnable application journeys](#runnable-application-journeys)
2. [Problem and model](#2-problem-and-model)
3. [Goals and non-goals](#3-goals-and-non-goals)
4. [Quick tour (normative conceptual sketch)](#4-quick-tour-normative-conceptual-sketch)
5. [Core concepts](#5-core-concepts)
6. [Schema declarations](#6-schema-declarations)
7. [Wire layout](#7-wire-layout)
8. [Generated access API](#8-generated-access-api)
9. [Type-validity checking](#9-type-validity-checking)
10. [Mutation and type preservation](#10-mutation-and-type-preservation)
11. [Tagged-union representation](#11-tagged-union-representation)
12. [Alignment, slots, and padding](#12-alignment-slots-and-padding)
13. [Composition and implementation boundaries](#13-composition-and-implementation-boundaries)
14. [Memory-safety argument](#14-memory-safety-argument)
15. [Stable snapshots and concurrency](#15-stable-snapshots-and-concurrency)
16. [Performance model](#16-performance-model)
17. [C and C++ interoperability](#17-c-and-c-interoperability)
18. [Errors](#18-errors)
19. [Architecture](#19-architecture)
20. [Test strategy](#20-test-strategy)
21. [Evolution](#21-evolution)
22. [Initial scope, limitations, and roadmap](#22-initial-scope-limitations-and-roadmap)
23. [Full example (normative conceptual sketch)](#23-full-example-normative-conceptual-sketch)
24. [Appendix A: attribute reference](#appendix-a-attribute-reference)
25. [Appendix B: schematic macro expansion](#appendix-b-schematic-macro-expansion)
26. [Appendix C: source references](#appendix-c-source-references)
27. [Final design summary](#final-design-summary)

---

## 1. Abstract

`zero-schema` is a `#![no_std]` library and a `zero-schema-macros` companion for
exact C-compatible layouts. A `#[zero]` declaration remains an ordinary logical
Rust struct or enum and also produces private `*Wire` storage, checked `*Ref`
capabilities, constrained `*Mut` capabilities, and a `*Patch` value for partial
logical updates.

`access` proves an existing aligned byte span is type-valid before returning a
small read capability. `access_mut` proves the same facts before returning an
exclusive mutation capability. Neither creates a record from bytes. There is no
public raw-wire reference, raw union mutation, delayed validity checkpoint, or
Rust-originated initialization path.

The API names aggregate movement by direction relative to the checked wire
capability. `copy_into` moves checked wire into the concrete logical return type
named by its signature or type inference; `copy_from` moves a logical patch or
slice into its checked mutable receiver. The `copy` prefix visibly warns that
complete aggregate work occurs. Thus `TypeRef::copy_into()` materializes a
logical `Type` out of checked wire, `TypeMut::copy_from(&TypePatch)` transfers
present logical fields into an already valid wire, `ArrayRef::copy_into()`
materializes its complete logical array, and `ArrayMut::copy_from(&[LogicalT])`
transfers an exact-length slice into its checked wire array. These operations
allocate nothing. Borrowed strings and fixed bytes in a logical result continue
to borrow the checked source.

Ordinary field access is zero-copy and field-named: `message.sequence()`,
`message.header()`, and `message.samples()`. Mutation starts from a field-named
short reborrow capability, such as `message.sequence_mut().set(43)?`. Aggregate
and field-local operations are deliberately separate: there are no root-level
field setters and no checked-view-to-mutable-wire transfer shortcuts.

Every tagged-union field is externally tagged. The containing record supplies
one unique matching scalar-enum sibling selected with `#[zero(tag_field = ...)]`.
A union capability privately coordinates that sibling tag and its payload; it is
never a stand-alone wire root.

### Runnable application journeys

The following maintained sources are the executable counterparts for the
supported 0.1 application journeys. They exercise reviewed producer bytes or
receiving storage; no fixture is encoded through generated APIs.

| Executable journey | Normative coverage in this RFC |
|---|---|
| [`records.rs`](../examples/records.rs) | Record lifecycle: exact access, zero-copy reads, explicit `copy_into`, constrained patch `copy_from`, and fresh access (§§4, 8.1–8.7, 9–10). |
| [`strings.rs`](../examples/strings.rs) | Borrowed `str`, `CStr`, `U16Str`, `U16CStr`, and fixed bytes; constrained field mutation and fresh access (§§6.4, 8.2, 8.4, 9–10). |
| [`tagged.rs`](../examples/tagged.rs) | Required external tag/payload coupling, selected reads, mutation, and switching (§§6.9–6.10, 7.5, 8.8, 11). |
| [`optional.rs`](../examples/optional.rs) | Zero-sentinel optional reads, `OptionMut` updates, tri-state patches, and fresh access (§§6.5, 7.4, 8.3, 8.6, 10). |
| [`access_errors.rs`](../examples/access_errors.rs) | Eager rejection of malformed producer bytes and structured access diagnostics (§§9, 18). |
| [`generic_receiving_buffer.rs`](../examples/generic_receiving_buffer.rs) | Fully concrete generic roots, aligned `schema_buffer!` receiving-storage types, and exact access (§§6.11, 8.9, 12). |
| [`no_std_wasm.rs`](../examples/no_std_wasm.rs) | Freestanding `no_std` wasm producer-byte access (§§1, 19.5). |

The sources are the executable tutorial material. The remaining RFC content is
reference-only normative specification: private wire/capability internals,
declaration constraints, target-specific C/C++ layout requirements, performance,
and safety arguments remain here. In particular, §17 specifies layout
requirements; it does not specify generated C++ declarations.

## 2. Problem and model

### 2.1 Shared-memory layouts are physical storage

A representative producer layout is fixed-size, naturally aligned, and directly
placed in shared memory:

```cpp
struct MemoryConfig {
    std::uint64_t capacity_bytes;
};

struct FileConfig {
    std::uint32_t flags;
    char path[260];
};

union ConfigPayload {
    MemoryConfig memory;
    FileConfig file;
};

struct alignas(64) Message {
    std::uint64_t sequence;
    std::uint16_t name_len;
    char name[64];
    std::uint16_t config_kind;
    ConfigPayload config;
};
```

This is target-ABI storage, including native scalar forms, padding, fixed
capacity, and union-sized payload bytes. An ordinary Rust mirror cannot safely
interpret arbitrary incoming storage as a `bool`, a closed enum, a borrowed
string, or a data-carrying enum. The safe boundary must establish those facts
before it forms a Rust-facing value or reference.

### 2.2 A checked capability is the boundary

```text
existing aligned, initialized bytes
        │
        ▼
Schema::access / Schema::access_mut
        │  exact layout and eager type-validity proof
        ▼
RecordRef / RecordMut
        │  field-named zero-copy capabilities and constrained writes
        ├───────────────────────────────────┐
        │                                   │
        ▼                                   ▼
checked nested / array / union views     logical value / logical patch
        │                                   │
        └────────── copy_into / copy_from ──┘
```

A root capability carries a checked location, not a materialized record. A
nested field produces a nested capability, an array field produces a fixed-size
view, and a union field produces a capability that has already checked its
external sibling tag and selected payload. Capabilities are compact opaque
handles; public storage never contains one field proxy per record field.

### 2.3 Zero-copy fast path and explicit aggregate movement

Zero-copy means ordinary safe access through `*Ref`, `*Mut`, nested
capabilities, `ArrayRef`, `ArrayMut`, and iteration never constructs a
record-sized aggregate. A primitive read is a checked load; a nested read is a
small derived capability. Aggregate movement is explicit and has a cost
proportional to the declared fields or elements it visits.

The crate consumes bytes that already exist. Logical materialization is not
deep ownership: declared borrowed strings and fixed bytes retain their source
borrow. Logical patches are caller-owned values; constructing one does not prove
that its content fits a destination wire.

## 3. Goals and non-goals

### 3.1 Goals

The initial design provides:

- item-owning `#[zero]` declarations retained as ordinary logical records,
  scalar enums, and closed tagged enums;
- `Schema::access(&[u8]) -> Result<SchemaRef<'_>, _>` and
  `Schema::access_mut(&mut [u8]) -> Result<SchemaMut<'_>, _>`;
- eager checking before a read or mutable capability is returned;
- compact, opaque root capabilities and temporary short-reborrow field mutation
  capabilities rather than stored public field proxies;
- field-named reads and field-named mutation entry points;
- allocation-free `copy_into()` aggregate materialization and all-or-nothing
  `copy_from(&SchemaPatch)` logical patch application;
- generated `SchemaPatch`, `Default`, and `From<Schema> for SchemaPatch`;
- native, little, and big endian direct scalar storage;
- fixed-width primitives, checked Booleans, closed scalar enums, bounded
  strings, fixed bytes, nested schemas, nonzero fixed arrays, and closed
  externally tagged unions;
- exact C field order, alignment, padding, array stride, and union payload
  shape;
- private `*Wire` representations formed through `zerocopy` and checked byte
  operations;
- generated allocation-free access and mutation errors;
- `SCHEMA_SIZE`, `SCHEMA_ALIGN`, `SCHEMA_STRIDE`, layout metadata, named
  `schema_buffer!` types, and `make_schema_buffer!` values;
- no handwritten `unsafe` in runtime, macro crate, or emitted implementation;
  and
- `#![no_std]` core support and C/C++ ABI verification.

### 3.2 Non-goals

The initial design does not provide:

- construction of an initial valid wire from Rust values;
- public `*Wire` storage, mutable raw bytes through a capability, raw union
  storage, or direct raw tag mutation;
- public field-proxy structs stored in each root capability;
- application/domain validation beyond type validity and representation bounds;
- checked-reference or checked-view direct transfer into mutable wire;
- range array transfers, partial slice transfer, or arbitrary iterator transfer;
- a root entry point for a tagged enum, including an aligned buffer for one;
- dynamic sequences, zero-length fixed arrays, relative offsets, packed
  records, atomic field operations, or automatic in-place migration;
- open scalar enums, open tagged unions, unknown-payload preservation, or
  shared external tags; or
- automatic proof that independently authored C++ declarations match a schema.

A successful capability proves the precise facts needed by its declared safe
operations. It does not certify business rules such as acceptable paths,
monotonic counters, or cross-record consistency.

## 4. Quick tour (normative conceptual sketch)

**Normative conceptual sketch; not standalone runnable code.** The declarations
and fragments below specify the capability relationships, but deliberately use
placeholder producers and helpers, and omit the reviewed fixture and surrounding
program. Use the [runnable application journeys](#runnable-application-journeys)
for executable producer-byte workflows.

### 4.1 Declaration

```rust
use zero_schema::zero;
use widestring::{U16CStr, U16Str};

#[zero(endian = "native")]
#[repr(u16)]
pub enum ConfigKind {
    Memory = 1,
    File = 2,
}

#[zero(endian = "native")]
pub struct Header<'buf> {
    #[zero(capacity = 32)]
    pub producer: &'buf core::ffi::CStr,
}

#[zero(endian = "native")]
pub struct MemoryConfig {
    pub capacity_bytes: u64,
}

#[zero(endian = "native")]
pub struct FileConfig<'buf> {
    pub flags: u32,
    #[zero(capacity = 260)]
    pub path: &'buf core::ffi::CStr,
}

#[zero(endian = "native")]
pub enum Config<'buf> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'buf>),
}

#[zero(endian = "native", align = 64)]
pub struct Message<'buf> {
    pub sequence: u64,
    pub header: Header<'buf>,
    #[zero(capacity = 64, len_type = u16)]
    pub name: &'buf str,
    pub samples: [u32; 3],
    pub config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    pub config: Config<'buf>,
}
```

The macro retains each declaration as its public logical value type. It emits a
separate private wire representation and capabilities. `Config` names variants
and payload layout, but it has no independent tag location and therefore no
independent root access surface.

### 4.2 Reading existing producer bytes

```rust
let bytes: &[u8] = producer_supplied_slot();
let message = Message::access(bytes)?;

let sequence = message.sequence();
let producer = message.header().producer();
let first_sample = message.samples().get(0);
let config = message.config();
let tag = config.tag();

if let Some(file) = config.file() {
    consume((tag, file.flags(), file.path()));
}

let value: Message<'_> = message.copy_into();
```

`access` establishes the full type-validity proof, so all shown reads are
infallible. `samples()` returns an O(1)-state `ArrayRef` unless the caller asks
for its complete logical array with `copy_into()`.

### 4.3 Mutating existing type-valid bytes

```rust
let bytes: &mut [u8] = exclusively_owned_producer_slot();
let mut message = Message::access_mut(bytes)?;

message.sequence_mut().set(43)?;
message.name_mut().set("replacement")?;
message.samples_mut().set(1, 21)?;
message.header_mut().producer_mut().set(c"patched producer")?;

let patch = MessagePatch {
    sequence: Some(44),
    name: Some("copied through a patch"),
    ..MessagePatch::default()
};
message.copy_from(&patch)?;
```

`access_mut` requires a pre-existing type-valid wire. It is not an initializer.
A direct operation preflights its input before its bounded write. `copy_from`
preflights all present patch entries before its first write, so it changes all
requested fields or none.

### 4.4 Optional aligned receiving storage

```rust
use zero_schema::{make_schema_buffer, schema_buffer};

type MessageBuffer = schema_buffer!(Message);
let mut slot: MessageBuffer = make_schema_buffer!(Message);
receive_producer_bytes(slot.as_bytes_mut())?;
let message = Message::access(slot.as_bytes())?;
```

`schema_buffer!(Message)` names correctly aligned Rust receiving storage with
exactly `Message::SCHEMA_SIZE` initialized bytes; `make_schema_buffer!(Message)`
constructs that type. Neither performs a validity proof or initializes a logical
schema: callers must use `access` to decide whether the current bytes are valid. A
schema whose complete
zero bytes satisfy every declared rule (for example, an all-optional root) can
therefore access as valid absence; other schemas still require producer data.

## 5. Core concepts

### 5.1 Logical schema value

A declaration such as `Message<'a>` is an ordinary logical Rust value with its
declared fields, variants, derives, literals, and patterns. It is not a wire
layout or a safe interpretation of arbitrary bytes. A copied logical aggregate
owns copied scalars, arrays, nested values, and enum shape; borrowed fields keep
the source borrow.

### 5.2 Private wire representation

The preceding `Message` conceptually uses this private storage shape:

```rust
#[repr(C, align(64))]
struct MessageWire {
    sequence: U64NativeWire,
    header: HeaderWire,
    name: StrWire<U16NativeWire, 64>,
    samples: [U32NativeWire; 3],
    config_kind: U16NativeWire,
    config: ConfigWire,
}
```

The actual emitted representation includes ABI-required padding. It is private:
callers cannot obtain a `MessageWire` reference, a mutable `MessageWire`
reference, a raw pointer, or a raw union member.

### 5.3 Compact read capability

`MessageRef<'wire>` is a small opaque capability over a fully checked root
location. It stores no decoded aggregate, array, payload cache, validity bitmap,
or public field-proxy collection. Field-named methods derive scalar values,
borrows, or sub-capabilities directly from the checked location.

A nested `HeaderRef`, `ArrayRef`, and `ConfigRef` are likewise short capability
values over selected checked storage. They are created on demand and may be
cheaply discarded. `copy_into()` is the explicit point at which a logical
aggregate is constructed.

### 5.4 Mutable capability and reborrows

`MessageMut<'wire>` is an exclusive capability over the same kind of checked
location. Its reads that expose a wire borrow are tied to the current shared
reborrow, not the full input lifetime. Its field mutation methods return
short-lived, field-local capability handles that hold the mutable reborrow only
while used.

For example, `message.name()` returns a string tied to the `&message` reborrow,
and `message.name_mut()` returns a `StringMut<'_>`. While a returned mutation
handle exists, Rust prevents another use that would overlap that mutable field
borrow. A handle offers only operations appropriate to that field. No public
capability exposes `&mut [u8]`, mutable private storage, or arbitrary payload
bytes.

### 5.5 Patch value

`MessagePatch<'a>` has one optional logical entry for every ordinary patchable
field, subject to external-tag coupling rules. `Default` makes each entry absent;
an absent entry retains the destination. `From<Message<'a>> for MessagePatch<'a>`
moves the logical fields into present entries. It performs no wire copy and no
representation validation.

### 5.6 Type validity

A wire is type-valid for a declaration when:

- the root span has exact size and sufficient alignment;
- each direct scalar can form its declared Rust-facing scalar type;
- each `bool` encoding is `0` or `1`;
- each scalar-enum discriminant names a declared variant;
- each bounded string length and active encoding is valid for its capacity;
- each C-string form has an in-capacity terminator;
- each external union tag names a declared case and that selected payload is
  recursively type-valid; and
- nested schemas and every fixed-array element are recursively type-valid.

Padding, unused bounded-string capacity, and inactive union payload bytes are
not logical fields and are not inspected.

### 5.7 Stable snapshot

A capability is sound only while its original bytes remain allocated, aligned,
initialized, and stable for the duration of the borrow. Ownership, publication,
coordination, and reclamation belong to the embedding system.

## 6. Schema declarations

### 6.1 Entry point and accepted items

`#[zero(...)]` is unconditionally re-exported by `zero-schema`, which unconditionally
depends on `zero-schema-macros`; `#[zero_schema::zero(...)]` is also valid. The
declaration surface is available with default features, with `alloc` alone, and with
default features disabled; it enables neither `alloc` nor `std` and does not alter the
target binary. The macro accepts a module-scope named-field struct, a fieldless scalar
enum, or a tagged enum. It replaces the complete input item and consumes nested
`#[zero(...)]` attributes. Function-local schemas are unsupported. Raw identifiers are
accepted and emitted public method names omit `r#`.

A tagged enum is a logical declaration plus a payload-layout declaration. It is
not a standalone schema root: it has no `access`, no `access_mut`, no standalone
buffer, and no independent wire tag. It becomes wire-reachable only through an
externally tagged field in a containing record.

### 6.2 Source-level value type

The macro retains the apparent declaration as an ordinary logical struct or
enum after consuming its nested options. Struct literals, direct field access,
pattern matching, and valid user derives remain available. Logical construction
does not prove bounded strings fit a wire, patches are mutually consistent, or a
union switch is complete; those representation checks occur during `copy_from`.
Handwritten implementations of generated wire support are outside the safety
promise.

### 6.3 Container options

```rust
#[zero(
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    crate = path::to::zero_schema,
    borrow = 'lifetime,
)]
```

| Option | Values | Default | Meaning |
|---|---|---:|---|
| `endian` | `"native"`, `"little"`, `"big"` | native | default direct-scalar representation |
| `align` | supported power of two | natural | raises record wire alignment |
| `crate` | runtime path | resolved | overrides the `zero_schema` crate path |
| `borrow` | DSL lifetime name | inferred | selects source-buffer lifetime |

A parent endian setting does not change a nested schema's declared form. Unknown,
duplicate, contradictory, misplaced, or inapplicable options are macro errors.

### 6.4 Field categories

| Source field | Wire form | Field method result |
|---|---|---|
| direct integer or float | selected-endian scalar wire | native Rust scalar |
| `bool` | `BoolWire` | `bool` after encoding check |
| scalar enum | raw integer wire | declared Rust enum after discriminant check |
| `&str` | length plus `[u8; N]` | `&str` |
| `&CStr` | `[u8; N]` | `&CStr` |
| `&U16Str` | native-endian unit storage | `&U16Str` |
| `&U16CStr` | native-endian unit storage | `&U16CStr` |
| `&[u8; N]` | `[u8; N]` | `&[u8; N]` |
| nested schema | inline child `*Wire` | `ChildRef<'_>` |
| `Option<T>` | complete `T` StorageWire; no presence byte | `Option<inner read>` |
| `[T; N]` | `[T::Wire; N]` | `ArrayRef<'_, T, N>` |
| tagged union | sibling scalar tag plus union-sized payload | `UnionRef<'_>` |

`String`, `Vec<T>`, `Box<T>`, maps, unsized arrays, arbitrary tuples, raw
pointers, and standard-library containers are unsupported. `Option<T>` is
supported only by the zero-sentinel rules in §6.5. A path is a nested schema
only when its complete wire form is expressible.

### 6.5 Zero-sentinel `Option` fields

An optional field is a **field-level**, zero-sentinel `Option<T>`; it is not
Rust's in-memory `Option` layout and it adds no discriminant, tag, bitmap, or
presence byte. The only canonical spellings are `Option<T>`,
`core::option::Option<T>`, `::core::option::Option<T>`,
`std::option::Option<T>`, and `::std::option::Option<T>`. An alias, a path that
only ends in `Option`, a qualified `Self::Option`, or any form other than one
type argument is rejected rather than silently acquiring sentinel semantics.

The matrix below is normative. `ZeroInvalid` means the complete inner wire can
never represent a logical value when every byte is zero; it is the injectivity
condition that keeps `None` distinct from every `Some` value.

| Source form | Accepted | Conditions or reason |
|---|---:|---|
| `Option<ClosedEnum>` | yes | every explicit scalar-enum discriminant is nonzero |
| `Option<Child>` | yes | `Child: OptionalWireType`, including generated cross-crate children whose complete wire has `ZeroState = ZeroInvalid` |
| `Option<[Child; N]>` | yes | `N` is nonzero and `Child` is an eligible all-zero-invalid scalar enum or schema path |
| primitive integer or float, `bool` | no | an all-zero wire is a valid value |
| `&str`, `&CStr`, `&U16Str`, `&U16CStr`, `&[u8; N]` | no | an all-zero representation can be valid or has no injective absence proof |
| `Option<Option<T>>` | no | the one sentinel cannot encode two logical absence states |
| `[Option<T>; N]` or `Option<[primitive_or_bool; N]>` | no | elements cannot have independent sentinel state and primitive zero is valid |
| `Option<TaggedPayload>` | no | its external tag is outside the field span and cannot be made sentinel-consistent |

For these rules, generated `WireTypeSupport::ZeroState` is a sealed,
doc-hidden two-state calculation. `ZeroValid` means an all-zero complete wire
can be logical; `ZeroInvalid` means it cannot. Record state is the OR of its
physical field states, and a nonempty array has its element state. Optional
fields contribute `ZeroValid`. A tagged payload is `ZeroInvalid` only when
every variant payload is `ZeroInvalid`; a unit or zero-valid variant makes it
`ZeroValid`. `ZeroInvalid::Or<R>` remains `ZeroInvalid`; `ZeroValid::Or<R>` is `R`.
The public source rule is the
doc-hidden blanket refinement `OptionalWireType: WireTypeSupport<ZeroState =
ZeroInvalid>`; callers neither implement the marker nor receive a raw wire
escape hatch.

`#[zero(align = N)]` may raise an optional field's alignment. `capacity`,
`len_type`, `endian`, and `tag_field` are all rejected on an `Option` field.
The inner type supplies its own supported schema or enum representation.

### 6.6 Field options

```rust
#[zero(
    capacity = INTEGER,
    len_type = u8 | u16 | u32,
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    tag_field = sibling_identifier,
)]
```

| Option | Valid categories | Additional conditions |
|---|---|---|
| `capacity` | four borrowed string forms | required exactly once there and forbidden elsewhere |
| `len_type` | `str`, `U16Str` | `u8`, `u16`, or `u32`; capacity must fit |
| `endian` | direct numeric storage and prefixed string length | nested and scalar enum types retain their own form |
| `align` | any supported field | supported power of two; the only field option permitted on `Option<T>` |
| `tag_field` | every tagged-union field | required; names one unique matching scalar-enum sibling |

`tag_field` is mandatory for every tagged-union record field. The named sibling
must be in the same named-field record, have precisely the scalar enum type used
by that union's variant tags, and be referenced by exactly one union field. The
macro rejects a missing option, an unknown sibling, a non-enum or wrong-enum
sibling, and a tag sibling referenced by multiple union fields.

A shared tag cannot be supported by a local type-preserving switch: changing one
payload would change a tag that selects another payload's interpretation. That
would transiently or permanently invalidate the other union field. One tag,
one union field is therefore a required layout and mutation invariant.

### 6.7 Fixed arrays

A field `[T; N]` is allowed when `T` is an allowed scalar, scalar enum, or nested
schema and `N` is a nonzero layout-supported const expression. Its physical field
is exactly `[T::Wire; N]` with contiguous `size_of::<T::Wire>()` stride.

`samples()` returns an O(1)-state `ArrayRef<'_, T, N>`. `get(index)` and
`iter()` stay zero-copy. `copy_into()` explicitly constructs `[LogicalT; N]`.
Access proves every element in increasing index order before it returns the
parent capability.

### 6.8 Scalar enums

A scalar enum must be fieldless, use `repr(u8)`, `repr(u16)`, or `repr(u32)`, and
assign each variant a unique explicit discriminant. Incoming raw storage is
matched before a Rust enum value is formed. A raw discriminant absent from the
declaration prevents access.

### 6.9 Tagged-union DSL

```rust
#[zero(endian = "native")]
pub enum Config<'buf> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'buf>),
}

#[zero(endian = "native")]
pub struct Message<'buf> {
    pub config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    pub config: Config<'buf>,
}
```

The public enum defines logical variants and payload layouts. The containing
record owns the one physical tag location. A `ConfigRef` originates only from
`MessageRef::config()` and exposes `tag()`, `memory()`, and `file()`. Exactly one
payload method produces `Some` after access has succeeded. `ConfigMut` originates
only from `MessageMut::config_mut()`.

### 6.10 External tag coupling and patches

The tag sibling remains a readable scalar field, so `message.config_kind()` and
`message.config().tag()` produce the same checked logical discriminant. It is not
independently mutable: no mutable capability is emitted for a tag sibling and no
root-level tag setter exists. The union field capability alone coordinates the
tag and payload.

A record patch may omit both coupled entries, contain a union entry alone, or
contain both. A union-only entry derives its tag. A tag-only entry is rejected.
When both are present, their tags must agree. `From<Record> for RecordPatch`
keeps both ordinary logical fields present, and an inconsistent logical record
therefore fails during patch preflight rather than being normalized silently.

### 6.11 Lifetimes and generics

Borrowed fields use the declaration's selected source lifetime. The emitted wire
contains no reference or lifetime state. Type and const parameters remain only
where layout is expressible. Both receiving-storage macros require a fully
concrete root schema; neither accepts a tagged payload because it has no root wire
layout.

## 7. Wire layout

### 7.1 Direct scalars

Each primitive has a doc-hidden all-bit-valid wrapper with exact size, alignment,
and native load/store routines. Little and big endian select identically sized
byte-order wrappers. `bool` uses an all-bit-valid `u8` wrapper and is checked
before a Rust Boolean is formed. Scalar enums use raw integer storage; `f32` and
`f64` use ordinary all-bit-valid storage.

### 7.2 Bounded strings

An `str` field is a selected-endian length followed by `[u8; N]`. Access checks
that the length is representable and within capacity and validates UTF-8 in
exactly the active prefix. A C-string field is `[u8; N]` and requires an
in-capacity terminator. Native `u16` unit storage follows `widestring`
borrowed-view semantics. Bytes after a prefix or terminator are unused capacity.

### 7.3 Fixed bytes, arrays, and nested schemas

`&[u8; N]` borrows exact storage and needs no content test. A fixed array is
`[T::Wire; N]`, never `[T; N]`; Boolean elements, enum elements, and nested
elements receive their relevant checks. The first failed element reports an
indexed path. A nested field is inline child wire storage, not a pointer or a
logical child value; the parent uses checked offsets and recursively proves the
child.

### 7.4 Zero-sentinel `Option` representation

`Option<T>` uses exactly the complete declared physical field storage of `T` as
its `StorageWire`; it has no presence byte and no separately addressable
discriminant. The authoritative span is the matching `FieldDescriptor` range
`offset..offset + size`, not merely the inner value's member bytes.
`FieldDescriptor::is_optional()` identifies this protocol while its `kind()`
continues to describe the inner field form.

`None` is represented **if and only if every byte in that complete StorageWire
span is zero**. The scan and a `set(None)` clear include the inner representation,
its internal padding, and any padding introduced by that field's `align` wrapper.
They exclude parent inter-field padding and root trailing padding, even if those
bytes are adjacent in the enclosing `repr(C)` record. Consequently a nonzero
byte in field-local padding makes the field `Some`; the implementation then
eagerly proves the inner value. A zero-invalid inner whose value bytes are zero
therefore produces an inner validation error rather than being reclassified as
`None`.

Access scans this full span in declaration order. An all-zero span is immediately
proved absent; a nonzero span must pass the ordinary eager proof for `T` before
any capability is returned. Mutation clears the same exact full span for `None`.
For `Some(value)`, it preflights initialization before writing the value wire;
there is no general raw-storage initialization route. Generic
`LayoutDescriptor::padding()` remains diagnostic only and is never a
validation/initialization policy; optional-field storage is the documented
exception that derives its rule from `FieldDescriptor`, not parent padding.

### 7.5 External tagged-union payloads

A tagged union field has this conceptual parent layout:

```text
MessageWire {
    ...
    config_kind: ConfigKindWire,
    config: ConfigWire,
}
```

`ConfigWire` is C-layout union-sized and union-aligned all-bit-valid storage.
Its size is the greatest member size rounded for union alignment. The sibling
tag and payload follow ordinary parent C field order and alignment; they need
not be adjacent in source order.

Access validates the sibling scalar tag before it selects any payload view. An
unknown tag produces `UnknownUnionTag`; it cannot select arbitrary union bytes.
After a known tag is chosen, access forms and validates only that member-sized
payload view. Inactive payload bytes are not interpreted or inspected.

### 7.6 Ordinary parent padding

Wire structs use C field order. Compiler-inserted inter-field and trailing
padding are ABI storage, not declared fields and not validity input. Safe access
and mutation ignore initialized padding, unused string capacity, and inactive
payload bytes.

## 8. Generated access API

### 8.1 Root entry points and constants

```rust
impl Message {
    pub const SCHEMA_SIZE: usize;
    pub const SCHEMA_ALIGN: usize;
    pub const SCHEMA_STRIDE: usize;
    pub const LAYOUT: LayoutDescriptor;

    pub fn access(bytes: &[u8]) -> Result<MessageRef<'_>, MessageAccessError>;
    pub fn access_mut(bytes: &mut [u8])
        -> Result<MessageMut<'_>, MessageAccessError>;
}
```

The byte span must have exactly `SCHEMA_SIZE` bytes at an address meeting
`SCHEMA_ALIGN`. `SCHEMA_STRIDE` rounds wire size for aligned slot arrays.
`LAYOUT` is diagnostic and ABI-verification metadata, not an interpreted runtime
layout description. `access_mut` performs complete type checking and has no
alternate initialization route.

### 8.2 Field-named read API

Read operations use the source field name exactly:

```rust
message.sequence() -> u64
message.name() -> &str
message.header() -> HeaderRef<'_>
message.samples() -> ArrayRef<'_, u32, 3>
message.config() -> ConfigRef<'_>
message.config().tag() -> ConfigKind
message.config().file() -> Option<FileConfigRef<'_>>
message.profile() -> Option<ProfileRef<'_>>
```

They are infallible after their parent capability exists and never expose endian
wrappers, raw private storage, or a raw union member. For `TypeMut`, every result
that borrows wire bytes is bound to the invocation's shared reborrow:

```rust
impl<'wire> MessageMut<'wire> {
    pub fn name<'view>(&'view self) -> &'view str;
    pub fn header<'view>(&'view self) -> HeaderRef<'view>;
    pub fn samples<'view>(&'view self) -> ArrayRef<'view, u32, 3>;
    pub fn config<'view>(&'view self) -> ConfigRef<'view>;
    pub fn copy_into<'view>(&'view self) -> Message<'view>;
}
```

Thus a borrow returned from a mutable root cannot be treated as though it lived
for all of `'wire`; the compiler rejects later overlapping mutation while that
borrow remains live.

### 8.3 Optional fields and `OptionMut`

An immutable optional getter returns `Option<inner read>` and rescans its live
complete StorageWire span on every call; it caches neither presence nor a
validated inner capability. A mutable root exposes a short field-local handle:

The generated optional adapter is doc-hidden; its type parameter does not expose
storage. A generated `profile_mut<'view>(&'view mut self)` returns
`OptionMut<'view, Profile, Adapter>`. The public runtime shape is:

```rust
impl<'view, LogicalT, Adapter> OptionMut<'view, LogicalT, Adapter>
where
    Adapter: OptionFieldAdapter,
{
    pub fn get(&self) -> Option<Adapter::Read<'_>>;
    pub fn get_mut(&mut self) -> Option<Adapter::Mut<'_>>;
    pub fn set<'source>(
        &mut self,
        value: Option<Adapter::Value<'source>>,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError>;
}
```

`get()` and `get_mut()` return `None` for an all-zero span. A nonzero span was
proved eagerly and returns the typed inner read or mutable capability. The
`'view` mutable reborrow prevents another use of the root while the handle or a
child from `get_mut()` lives. `set(None)` clears exactly the complete StorageWire
span; `set(Some(value))` preflights the inner initialization before committing
the value wire. Neither method exposes a byte slice, wire value, or initializer
for an arbitrary root.

### 8.4 Field-local mutation API

A mutable field method returns a temporary capability tailored to that field:

```rust
impl<'wire> MessageMut<'wire> {
    pub fn sequence_mut<'view>(&'view mut self) -> ScalarMut<'view, u64>;
    pub fn name_mut<'view>(&'view mut self) -> StringMut<'view>;
    pub fn header_mut<'view>(&'view mut self) -> HeaderMut<'view>;
    pub fn samples_mut<'view>(&'view mut self) -> ArrayMut<'view, u32, 3>;
    pub fn config_mut<'view>(&'view mut self) -> ConfigMut<'view>;
}
```

Scalar and string handles provide field-local `set`:

```rust
message.sequence_mut().set(43)?;
message.name_mut().set("replacement")?;
message.header_mut().producer_mut().set(c"producer")?;
```

Nested mutable capabilities repeat this pattern, so mutations naturally chain
through checked child locations. There is no root-level field-setting operation,
no stored public proxy per field, and no mutable handle for the sibling tag of an
externally tagged union.

### 8.5 Logical aggregate movement

Aggregate operations use only two directional names. `copy_into` moves checked
wire into a logical value, while `copy_from` moves a logical patch or exact
array slice into checked mutable wire. The `copy` prefix is the visible
aggregate-work warning:

```rust
impl<'wire> MessageRef<'wire> {
    pub fn copy_into(&self) -> Message<'wire>;
}

impl<'wire> MessageMut<'wire> {
    pub fn copy_into<'view>(&'view self) -> Message<'view>;

    pub fn copy_from<'value>(
        &mut self,
        patch: &MessagePatch<'value>,
    ) -> Result<(), MessageMutationError>;
}
```

The same pair applies to every nested schema capability:

```rust
impl<'wire> HeaderRef<'wire> {
    pub fn copy_into(&self) -> Header<'wire>;
}

impl<'wire> HeaderMut<'wire> {
    pub fn copy_into<'view>(&'view self) -> Header<'view>;
    pub fn copy_from(
        &mut self,
        patch: &HeaderPatch<'_>,
    ) -> Result<(), MessageMutationError>;
}
```

`copy_into` copies from checked wire into the concrete logical return type
identified by the method signature or type inference. It copies scalars, arrays,
nested aggregates, and the selected enum shape; declared borrowed contents
remain borrows. `copy_from` copies from a logical patch into the checked mutable
receiver. It is the complete and partial logical record-update operation.

The explicit Ref-to-Mut path is therefore:

```rust
let value = source.copy_into();
let patch = MessagePatch::from(value);
destination.copy_from(&patch)?;
```

No checked reference or view is directly transferred into mutable wire. This
keeps movement explicit at the logical boundary and gives every destination
update the same preflight rules.

### 8.6 Patches and all-or-nothing preflight

```rust
#[derive(Default)]
pub struct MessagePatch<'a> {
    pub sequence: Option<u64>,
    pub header: Option<HeaderPatch<'a>>,
    pub name: Option<&'a str>,
    pub samples: Option<[u32; 3]>,
    pub config_kind: Option<ConfigKind>,
    pub config: Option<ConfigPatch<'a>>,
}

impl<'a> From<Message<'a>> for MessagePatch<'a> {
    fn from(value: Message<'a>) -> Self {
        Self {
            sequence: Some(value.sequence),
            header: Some(HeaderPatch::from(value.header)),
            name: Some(value.name),
            samples: Some(value.samples),
            config_kind: Some(value.config_kind),
            config: Some(ConfigPatch::from(value.config)),
        }
    }
}
```

`None` retains the destination. A present ordinary field requests a logical
transfer. Before writing any byte, `copy_from` validates every present field
recursively: string capacities and encodings, every array member, nested patches,
union-patch completeness, and tag/union coupling. On failure it writes nothing;
on success it writes every present entry and leaves absent entries untouched.

For every optional field, the generated patch entry is `Option<Option<P>>`, where
`P` is the inner scalar-enum or schema patch, or the complete logical array. The
two layers deliberately distinguish patch absence from logical absence:

| Patch entry | Destination state | Required action |
|---|---|---|
| `None` | absent or present | retain unchanged |
| `Some(None)` | absent or present | clear the complete StorageWire span to zero |
| `Some(Some(P))` | present | apply the ordinary partial inner patch or full array update |
| `Some(Some(P))` | absent | initialize a complete `P` only |

An absent optional field cannot be promoted from a partial schema patch:
`P` must contain every inner logical field required for initialization. During
the declaration-order preflight pass, an incomplete absent promotion reports
`IncompleteOptionalInitialization` before any inner capacity or child
preflight for that field. Every patch entry—including a clear—is committed only
after the whole patch preflight succeeds. Therefore any error leaves all
destination bytes unchanged; a successful clear or promotion preserves
type-validity without a staging allocation.


`From<Type> for TypePatch` does not conflict with `copy_from`: it only moves
logical fields into present patch entries. It neither allocates nor performs a
wire copy. No `From<&Type>` is required, so a large logical value is not cloned
implicitly.

### 8.7 Array references and exact full-slice movement

`ArrayRef<'a, T, N>` has O(1) state over a fully checked wire array:

```rust
impl<'wire, LogicalT, const N: usize> ArrayRef<'wire, LogicalT, N> {
    pub fn get(&self, index: usize) -> Option<LogicalT>;
    pub fn iter(&self) -> ArrayRefIter<'wire, LogicalT, N>;
    pub fn copy_into(&self) -> [LogicalT; N];
}

impl<'wire, LogicalT, const N: usize> ArrayMut<'wire, LogicalT, N> {
    pub fn copy_from(
        &mut self,
        values: &[LogicalT],
    ) -> Result<(), MessageMutationError>;
}
```

```rust
let samples = message.samples();
let first = samples.get(0);
for sample in samples.iter() {
    consume(sample);
}
let values: [u32; 3] = samples.copy_into();
```

`ArrayMut<'a, T, N>` offers indexed direct operations and exact full-slice
movement:

```rust
let mut samples = message.samples_mut();
samples.set(1, 21)?;
let element = samples.get_mut(2);
samples.copy_from(&[10, 20, 30])?;
```

`copy_from(&[LogicalT])` requires exactly `N` elements. It preflights every source
element in increasing index order before its first destination write, so an
invalid later element leaves the entire array unchanged. Arrays intentionally
provide no range operation, no checked-view movement, and no checked-element
movement. `get_mut(index)` returns a constrained mutable nested-element
capability when `T` is nested; it never returns mutable private wire storage.

### 8.8 Union capabilities and patch movement

A union field capability privately owns the protocol for its sibling tag plus
payload. The read surface is selected-payload only:

```rust
let config = message.config();
match config.tag() {
    ConfigKind::Memory => config.memory().unwrap().capacity_bytes(),
    ConfigKind::File => config.file().unwrap().flags(),
}
```

Union capabilities use the identical pair as records and nested schemas:

```rust
impl<'wire> ConfigRef<'wire> {
    pub fn copy_into(&self) -> Config<'wire>;
}

impl<'wire> ConfigMut<'wire> {
    pub fn copy_into<'view>(&'view self) -> Config<'view>;
    pub fn copy_from(
        &mut self,
        patch: &ConfigPatch<'_>,
    ) -> Result<(), MessageMutationError>;
}
```

A mutable union capability also exposes the currently selected payload:

```rust
{
    let mut config = message.config_mut();
    if let Some(file) = config.file_mut() {
        file.flags_mut().set(0x20)?;
    }
}
message.config_mut().copy_from(&ConfigPatch::File(FileConfigPatch {
    flags: Some(0x40),
    path: None,
}))?;
```

`ConfigPatch` is a generated logical patch enum. A partial patch may change only
the currently selected variant. A switch requires a complete patch for the newly
selected payload because inactive bytes do not provide missing logical fields.
A successful switch writes and validates its new payload before committing the
external sibling tag last. There is no public tag mutation capability, no
independent tag setter, and no operation accepting untyped payload storage.

### 8.9 Aligned receiving storage

`schema_buffer!(FullyConcreteSchema)` names the public `SchemaBuffer<Wire, N>`
type whose byte view has `SCHEMA_SIZE` bytes at `SCHEMA_ALIGN`;
`make_schema_buffer!(FullyConcreteSchema)` constructs an initialized value of
that type. Callers may hand its mutable bytes to an external producer, but neither
macro establishes type validity.

## 9. Type-validity checking

### 9.1 Ordered check sequence

`access` and `access_mut` use the same eager order:

1. check exact root length and alignment;
2. form a private root wire view through `zerocopy`;
3. inspect declared fields in declaration order;
4. check constrained primitive encodings and scalar enum discriminants;
5. check bounded string lengths, UTF-8, and terminators;
6. check fixed-array elements in increasing index order;
7. recursively check nested schemas; and
8. for every union field, check its external sibling tag and exactly the selected
   payload.

For unchanged input the first failure is deterministic. Root layout failure
precedes field failure; field order precedes later fields; an unknown external
tag precedes its payload failure. Ordinary parent padding, unused capacity, and
inactive payload bytes are not read; §7.4 is the precise optional-field exception.

### 9.2 Exact boundary

| Incoming condition | Result | Reason |
|---|---|---|
| wrong root length or insufficient alignment | error | a typed root location cannot be formed safely |
| Boolean encoding other than `0` or `1` | error | no declared Rust Boolean may be returned |
| scalar enum value absent from declaration | error | no declared enum value may be returned |
| string length exceeds capacity or length form | error | bounded field view cannot be formed |
| active string bytes are not UTF-8 | error | no declared string borrow may be returned |
| C-string form lacks an in-capacity terminator | error | no declared C-string borrow may be returned |
| external union tag absent from declaration | error | selected payload is unknown |
| selected payload, array element, or nested field invalid | error | its declared view cannot be exposed safely |
| complete StorageWire of `Option<T>` | scanned | all zero is absent; nonzero eagerly proves `T` |
| ordinary parent padding, unused capacity, inactive payload bytes | ignored | no declared operation reads them |

### 9.3 Why checking is eager

Capabilities retain no validity bitmap, decoded aggregate, string-length cache,
array cache, or payload cache. Eager proof keeps field methods infallible inside
the valid borrow and leaves the capabilities compact. The bounded cost belongs to
entry access, not every field read.

### 9.4 Checked slot selection

A slot array may use `SCHEMA_STRIDE` after checked arithmetic:

```rust
let offset = index.checked_mul(Message::SCHEMA_STRIDE)
    .ok_or(Error::OffsetOverflow)?;
let limit = offset.checked_add(Message::SCHEMA_SIZE)
    .ok_or(Error::OffsetOverflow)?;
let message = Message::access(&mapping[offset..limit])?;
```

The mapping base and stride must make the chosen location aligned. A byte
subslice is not presumed aligned merely because its length is correct.

## 10. Mutation and type preservation

### 10.1 Entry condition

`access_mut` performs complete eager checking and returns `RecordMut` only for an
already type-valid wire. It cannot interpret an arbitrary mutable span as an
empty, default, or partially initialized record.

### 10.2 Operation order and atomicity

Every direct field-local operation:

1. identifies the declared field or selected payload through a private checked
   location;
2. validates every typed input, bound, encoding, and tag dependency it needs;
3. performs one bounded final store or bounded in-place transfer only after that
   preflight succeeds.

`TypeMut::copy_from(&TypePatch)` and `ArrayMut::copy_from(&[T])` strengthen that
rule across multiple destinations. They preflight their full requested logical
input before the first write. A union switch writes the complete new payload
first and its external sibling tag last. Therefore a successful operation
preserves the whole type-valid wire; an error preserves every prior byte.

### 10.3 Nested, array, and union limits

Nested fields produce their own checked mutable capability. Direct leaf mutation
continues through field-named handles, and a full or partial nested logical update
uses the child capability's `copy_from`. Arrays permit indexed direct `set` and
`get_mut`, complete `copy_into`, and exact full-slice `copy_from` only. Unions
permit selected-payload mutation and logical patch switching only. No operation
can resize storage, alter byte order, pick raw union bytes, independently write
a coupled tag, or expose unrestricted mutable storage.

## 11. Tagged-union representation

### 11.1 One external tag per union field

A public data-carrying enum is a logical value, not arbitrary incoming wire
storage. Its discriminant can be invalid, its Rust layout is not the required C
union layout, and its payload may contain borrowed views. Each use of a tagged
enum as a record field must name one external sibling tag with `tag_field`.
There is one physical scalar tag location per union field and one union field per
such location.

The macro emits only this external-tag representation. A tagged enum alone
cannot be accessed, mutated, or buffered at the root because it cannot identify
its tag location. Only a containing record's union field capability supplies
that context.

### 11.2 Private payload representation

The macro emits a private C-layout union-sized payload representation so the
compiler calculates maximum member size and alignment. Its members are
`ManuallyDrop<VariantWire>` all-bit-valid wire forms. Generated code never
returns a union member. After tag proof, a checked byte-origin subspan identifies
the selected member-sized location and `zerocopy` forms the payload wire view.
No implementation path assumes an active Rust union member, transmutes a union,
or casts arbitrary aggregate bytes.

### 11.3 Coupled patch rules

For record patches, the tag sibling is coupled with its union field:

| Patch entries | Result |
|---|---|
| neither tag nor union | retain both |
| union only | derive and commit the union's tag |
| tag only | reject before writing |
| tag plus union with matching tag | accept |
| tag plus union with differing tag | reject before writing |

For a union patch, same-variant partial updates are allowed. A variant switch
requires a complete new payload patch. This rule prevents inactive bytes from
being misused as a missing source field and preserves all-or-nothing behavior
without staging a record-sized payload.

### 11.4 Unit variants and unknown tags

A unit payload has nonzero storage in the initial scope and participates in
ordinary union size and alignment calculations. Zero-sized roots, nested wire
fields, array elements, and union members are unsupported. A closed external tag
must be known before any payload view exists.

## 12. Alignment, slots, and padding

### 12.1 Root alignment

Safe root access requires the actual buffer address to meet `SCHEMA_ALIGN`.
`SCHEMA_STRIDE` is size rounded to that alignment for repeated slots.
`schema_buffer!` names aligned owned receiving storage and `make_schema_buffer!`
constructs it, but neither establishes type validity.

### 12.2 Field alignment

A field `#[zero(align = 32)]` uses a wire-aligned wrapper. Its offset satisfies
that alignment and its storage size is rounded as required. Nested wire alignment
propagates into the parent C layout.

### 12.3 Ignored bytes and the optional-field exception

Parent inter-field and root trailing padding are ABI storage, not declared fields.
Access and ordinary movement ignore their initialized contents, unused string
capacity, and inactive union bytes. `Option<T>` is different only for the exact
complete field span specified in §7.4: field-local alignment and internal
padding are presence-significant and `set(None)`/a patch clear zeroes them.

### 12.4 Packed layouts

Packed representations are outside the initial scope. They conflict with safe
native `u16` wide-string views and reference-backed nested wire forms. A packed
backend would need offset-based scalar and string views instead.

## 13. Composition and implementation boundaries

### 13.1 Per-schema composition

The runtime composes private wire representations, public logical values and
patches, and public capability names per declaration. Associated implementation
types are sealed or doc-hidden; custom handwritten wire support is outside the
safety promise. `Wire` is never an escape hatch.

### 13.2 Private access input and optional adapters

Generated code composes through private checked-input capabilities holding the
typed wire location and original byte bounds. They support checked offsets,
nested derivation, external-tag selection, and complete optional StorageWire
scans. The optional adapter, `StorageWire`, `ValueWire`, zero-state marker, and
private token are doc-hidden composition details; no public capability exposes
them, a raw byte range, or a wire reference. They are not public extension API.

### 13.3 Lifetimes and nested capabilities

A child wire type contains no source-lifetime state even when the logical child
has borrowed fields. A child from `TypeRef` borrows under the root input lifetime.
A child, string, array view, union view, or logical aggregate from `TypeMut`
borrows under that invocation's shared reborrow. This prevents a mutable root
from returning a read borrow that incorrectly outlives the shared borrow that
created it.

### 13.4 Recursion

Inline recursive layouts must be finite. The macro rejects forms whose complete
wire layout cannot have finite size. Nested schemas and fixed arrays are allowed
only when their complete wire forms are finite.

## 14. Memory-safety argument

### 14.1 Representation formation

Every generated `*Wire` is `repr(C)` or an exact scalar wrapper. Storage derives
the needed `zerocopy` traits through the consuming crate's resolved dependency.
Root access checks exact length and alignment before generated code forms a typed
view.

Runtime code, macro code, and emitted implementation contain no handwritten
`unsafe`, manual aggregate conversion assertions, transmutation, or arbitrary
pointer casts. Typed views and bounded byte inspection use `zerocopy` and checked
safe operations.

### 14.2 Rust values follow proof

| Declared result | Private storage | Fact established first |
|---|---|---|
| integer or float | all-bit-valid scalar wire | direct load representation is valid |
| `bool` | `BoolWire` | raw byte is `0` or `1` |
| scalar enum | raw integer wire | discriminant names a declared variant |
| `&str` | length plus bytes | prefix is bounded and UTF-8 |
| C-string / wide C-string | fixed units | an in-capacity terminator exists |
| nested capability or value | child wire | child conditions hold |
| array view or logical array | `[T::Wire; N]` | every element condition holds |
| union capability or logical enum | external tag plus payload | tag and selected payload conditions hold |

No arbitrary input is first interpreted as a Rust Boolean, scalar enum,
reference, string object, or data-carrying enum. `copy_into` runs only after the
capability's proof and therefore reuses those established facts.

### 14.3 Borrow provenance

Root, nested, array, string, and selected-payload references derive from checked
locations within original bytes. A `TypeRef` result carries that input lifetime.
A borrowing result from `TypeMut` is shorter: it carries the current shared
reborrow. Logical aggregates retain the corresponding borrows for borrowed
fields. No handwritten conversion manufactures or extends a reference.

### 14.4 Union selection preserves byte origin

The implementation preserves these invariants:

1. the root input covers the entire root wire;
2. emitted offset arithmetic is checked and every selected location is in bounds;
3. the external sibling tag is checked before payload selection;
4. the selected view uses only its member-sized byte-origin location; and
5. no pointer is reinterpreted as a different, non-selected payload type.

Const assertions establish relevant payload offsets, sizes, and alignment.

### 14.5 Mutation preserves type validity

Before a direct operation writes, it preflights every fallible source, input, and
selected-union condition. Whole patches and full array slices preflight the
complete request before their first write. Normal field handles write only valid
representations. A union patch writes payload before its coupled external tag.
Thus success preserves type validity and an error preserves the prior wire.

## 15. Stable snapshots and concurrency

### 15.1 Wire correctness and synchronization

A capability proves type validity when access checked it. It does not make
concurrent non-atomic mutation compatible with ordinary Rust or C++ references.
The embedding system must ensure a stable snapshot for the complete borrow.

### 15.2 Immutable-slot protocol

A suitable protocol is:

1. a producer exclusively owns an aligned slot;
2. it writes a type-valid wire with its own integration;
3. it publishes the completed slot with the required synchronization;
4. a reader obtains that stable slot and calls `access`; and
5. reclamation prevents slot reuse until all derived capabilities are gone.

Reference counting, epochs, RCU, or equivalent ownership can provide the final
step. The schema crate supplies neither producer nor synchronization.

### 15.3 Double buffers and seqlocks

Double or multi-buffered immutable handoff fits the model. A reader racing
ordinary non-atomic payload stores cannot make reference formation safe simply by
retrying. Such a design needs a specialized atomic or volatile reader or a
separately owned snapshot outside this API.

## 16. Performance model

### 16.1 Space

`RecordRef`, `RecordMut`, nested capabilities, `ArrayRef`, `ArrayMut`, and
union capabilities are pointer-sized or bounded by a small fixed number of
pointers. They carry no decoded record, allocation, public proxy collection,
validity bitmap, decoded array, or payload cache. Logical values and patches
exist only when explicitly constructed by the caller.

### 16.2 Access cost

Root length/alignment tests and root-view formation are O(1). Eager validity
cost is bounded by declaration capacities:

| Operation | Cost |
|---|---:|
| root size and alignment | O(1) |
| direct scalar / Boolean / scalar enum | O(1) |
| UTF-8 validation | O(active byte length) |
| narrow or wide terminator scan | O(capacity) worst case |
| external tag dispatch and selected payload | O(1) plus selected payload cost |
| fixed-array proof | O(`N`) plus nested element costs |
| nested proof | sum of child costs |

Unused capacity, ordinary parent padding, and inactive payload storage are not
scanned. An optional field instead scans its bounded complete StorageWire span.

### 16.3 Field, aggregate, and mutation cost

A primitive field read is a native load. A scalar handle performs typed preflight
and one final store; a string handle validates its bound and encoding before a
bounded transfer. Indexed array mutation performs one checked element operation.

`copy_into` visits each logical field and array element once and allocates
nothing. `ArrayRef::copy_into()` is O(`N`) plus nested materialization. Patch
`copy_from` performs a complete preflight pass over present entries followed by
bounded writes; exact full-slice `ArrayMut::copy_from` has the analogous two-pass
cost. These operations do not clear or compare ordinary parent padding, unused
capacity, or inactive payload bytes; an optional `None` clear zeroes its full span.

## 17. C and C++ interoperability

### 17.1 Layout profile

For same-host shared memory, native endian plus emitted `repr(C)` field order
and matching alignment is the recommended profile. Explicit-endian scalar fields
correspond in C or C++ to identically sized and aligned byte forms with helpers;
they are not ordinary host-order integers. Wire integers are fixed-width integer
storage only: C/C++ `bool`, enum storage, pointers, bitfields, and `wchar_t` are
not interchangeable protocol fields.

### 17.2 Producer obligations and optional sentinels

A C or C++ producer must supply an aligned initialized span with exact root size
and initialize **every transported byte**, including compiler padding, before
Rust forms a slice. It must write direct scalar storage in declared byte order,
valid Boolean and scalar-enum encodings, bounded strings, and for each union
field a declared sibling tag with its matching selected payload. When switching
an external union, initialize the selected payload before publishing its tag.

Inline C values have no universal `NULL`. The selected optional protocol is an
explicit all-zero **complete object representation**: to produce `None`, zero
the complete physical field storage, for example
`memset(&record.profile, 0, sizeof record.profile)` (equivalently,
`memset(field, 0, sizeof field)` when `field` denotes that complete field
storage). This is not pointer-null equivalence and does not permit a generic
assignment of `NULL` to an inline enum, struct, or array. To produce `Some`,
initialize a valid nonzero inner representation and every byte that crosses the
FFI boundary. Parent padding may be arbitrary only after it too is initialized;
it remains excluded from the optional presence scan.

### 17.3 C++ union shape

A C++ record contains scalar tag storage beside a C++ union of payload structs.
C++ code treats raw tag storage as an integer until it recognizes a declared
enumerator, then uses only the corresponding payload member. Exact-width integer
storage is required; `wchar_t` is not portable, and `char16_t` requires matching
layout and aliasing assertions.

### 17.4 Required assertions and compatibility guard

Support requires target-specific assertions for `sizeof`, `alignof`, relevant
`offsetof` values, scalar representations, array stride, optional field size,
tag field offsets, and payload size/alignment. `repr(C)` follows target ABI
conditions rather than a universal ABI. Compatibility remains guarded by the
target, compiler, ABI-changing flags, packing choices, endian profile, and these
assertions; it is never inferred from source spelling alone.

## 18. Errors

### 18.1 Generated error families

Each schema emits structured allocation-free access and mutation errors.
`MessageAccessError` covers root length/alignment, scalar encoding, scalar enum,
string capacity/encoding/terminator, indexed array, nested, external-tag, and
selected-payload failures. `MessageMutationError` covers reachable direct-field,
patch, full-slice length, nested-value, tag/union mismatch, incomplete optional
initialization, and incomplete union-switch preflight failures. It may wrap an
access error when a workflow begins with `access_mut`.

### 18.2 Failure contracts

`access` never returns a partly checked capability; `access_mut` never returns a
mutable capability unless its input was type-valid at entry. `copy_into` is
infallible after that proof. A field-local mutation error changes no byte. Patch
and full-array-slice movement apply that guarantee to the entire request: any
error leaves the complete prior wire unchanged.

No error category describes initialization of arbitrary mutable schema bytes,
because the API never treats such bytes as an initial record.

### 18.3 Paths and inspection

The runtime exposes operation-independent `ErrorKind`, `ErrorPathSegment`, and a
`SchemaError` trait with `kind`, `schema`, `segment`, and `child`. Consumers use
these stable inspection values rather than diagnostic text. Under `std`, concrete
errors also implement `std::error::Error`.

Parents wrap concrete child errors rather than erasing them. Formatting walks
static field, array-index, and variant segments without allocation, producing
paths such as:

```text
Message.samples[3]: unknown enum value
Message.config.File.path: missing terminator
```

## 19. Architecture

### 19.1 Crate split

```text
zero-schema/
    src/
        lib.rs
        access.rs
        mutation.rs
        array.rs
        tagged.rs
        strings.rs
        wire.rs
        error.rs
        layout.rs
        __private.rs

zero-schema-macros/
    src/
        lib.rs
        parse.rs
        analyze.rs
        layout.rs
        emit_access.rs
        emit_mutation.rs
        emit_patch.rs
        emit_wire.rs
        errors.rs
```

The runtime owns wire helpers, checked access inputs, compact capabilities,
logical materialization, patch movement, arrays, tagged unions, errors, and
metadata. The proc macro retains each logical declaration and emits its private
wire, access, mutation, and patch code.

### 19.2 DSL analysis and hygiene

The macro records item kind, source lifetime, generics, ordered fields or
variants, options and spans, field categories, dependencies, layout detail, and
emitted names. It validates every tagged-union field's mandatory unique external
tag relationship during analysis, before layout or method emission. It resolves
renamed runtime and direct `zerocopy` crates with `proc_macro_crate`; `crate =
...` changes only the runtime path.

`zero-schema-macros` uses procedural-macro host APIs while `zero-schema` remains
`#![no_std]`. It is an unconditional build-time dependency, so declaration entry is
available under every runtime feature selection; host macro execution enables neither
`alloc` nor `std` and does not affect target-binary behavior.

### 19.3 Wire primitive family and generated operations

The runtime centralizes all-bit-valid primitive storage, size/alignment,
`zerocopy` traits, and private native loads/stores. Bounded strings use
`StrWire`, `CStrWire`, `U16StrWire`, and `U16CStrWire`; the macro does not
recreate those primitives for each field.

Generated access checks fields in declaration order and returns capabilities from
checked locations. Generated aggregate materialization runs only after access
proof. Generated patch and array movement preflights whole logical inputs before
writing. Runtime code, macro output, and token templates contain no handwritten
`unsafe` or raw-access escape hatch.

### 19.4 Layout metadata

Each root schema emits `LayoutDescriptor` metadata for wire size, alignment,
stride, field offsets and sizes, optional-field flags, arrays, strings, scalar
enums, external tag relationships, variants, padding locations, and byte order.
It is diagnostic and verification metadata, not a stable serialized fingerprint
or header-emission protocol.

### 19.5 Features and allocation

The core runtime is `#![no_std]`. Access, mutation, aggregate movement,
complete optional-span scans and clears, `OptionMut`, capabilities, patches,
errors, and metadata allocate nothing. Optional `alloc` and `std` conveniences
do not change wire layout, the sentinel protocol, or the type-validity boundary.

## 20. Test strategy

### 20.1 Compile-time coverage

UI tests cover retained logical declarations and derives, missing capacity,
capacity/length bounds, unsupported shapes, `N = 0`, scalar discriminants,
zero-sentinel Option spelling and rejection matrix, union variant tags, generated
patch names, ambiguous lifetimes, invalid alignment, native-wide-string
requirements, generic constraints, and generated-name collisions.

Union diagnostics must specifically reject every tagged-union record field that
omits `tag_field`, names an absent sibling, names a sibling of a different scalar
enum type, names a non-scalar-enum sibling, or shares a sibling tag with a second
union field. They also verify a tagged enum cannot be used as a root schema.

### 20.2 Access and `copy_into` behavior

Behavioral tests use producer-supplied byte fixtures or a C++ harness. They cover
primitive values, Booleans, bounded strings including embedded NUL where
permitted, nested capabilities, arrays, zero-sentinel options, and externally
tagged unions with unit and unknown tags. They compare `copy_into` with field
reads for every supported logical record and union and compare array `copy_into`
with indexed reads.

Boundary tests alter initialized ordinary padding, unused string capacity, and
inactive payload storage in otherwise type-valid fixtures and require access and
logical materialization to succeed. Optional-field tests instead require an
all-zero complete span to read `None`, a nonzero span to prove `Some`, and
nonzero field-local padding with a zero-invalid value to fail. They also require
failure for raw Boolean `2`, an undeclared scalar enum discriminant,
over-capacity length, malformed UTF-8, missing terminator, unknown external
union tag, and invalid selected nested or array content.

### 20.3 Mutation and copy-from behavior

Every mutation test starts from a producer-supplied type-valid fixture, obtains
`access_mut`, and exercises scalar/string field handles, chained nested handles,
selected-payload handles, partial patches, complete patches, and full array
slices. Fresh access must succeed after each successful mutation.

`From<Type> for TypePatch` must present every applicable field. Applying that
patch reproduces each logical field read. Patch tests cover default no-op,
single-field, nested, complete, union-only derivation of the sibling tag,
matching paired entries, mismatched paired entries, rejected tag-only entry,
same-variant partial update, complete union switch, and rejected incomplete
switch. Every preflight error snapshots and requires byte-for-byte preservation.

Array tests cover indexed `set`, indexed `get_mut`, complete `copy_into`,
exact-length `copy_from`, and wrong-length or invalid-later-element rejection.
They verify all-or-nothing preservation. Optional tests cover `OptionMut`
`set(None)`/`set(Some)`, tri-state patch retain/clear/present update, rejected
partial absent promotion, and whole-patch atomicity. Union tests cover selected
mutation, patch switching, payload-before-tag ordering, absence of independent
tag mutation, and absence of raw payload access.

### 20.4 Arbitrary bytes, Miri, and implementation audit

Arbitrary initialized byte input must never cause undefined behavior or a panic
through safe access; invalid input returns a structured error. Miri covers root,
nested, array, and union access; short shared reborrows from mutable roots;
aggregate materialization; patches; full arrays; selected-payload mutation; and
alignment rejection. A source audit rejects handwritten `unsafe` blocks, unsafe
trait implementations, and unsafe operation bodies in runtime, macro crate, and
emitted templates.

### 20.5 Cross-language and cross-endian coverage

The C++ harness compares wire `sizeof`, `alignof`, relevant `offsetof`, scalar
forms, external tag fields, payload storage, padding tolerance, and rejection
statuses. Native zero-sentinel Option cases `1012`–`1016` run in the native default,
GCC, and Clang configurations. They are intentionally excluded from the frozen
10-ID foreign-profile golden set because those foreign profiles cannot truthfully
regenerate the native ABI cases. Cross targets or emulation cover explicit-endian
scalar forms. Wide string profiles cover native-endian acceptance and inappropriate
direct-view rejection.

## 21. Evolution

Fixed layouts evolve explicitly. Adding inline storage changes offsets, size, or
alignment and therefore requires a new schema, reserved storage, or a separately
identified slot protocol. Closed scalar enums and closed tagged unions reject
unknown raw values; adding a declared value is safe only after all readers
understand it.

`LayoutDescriptor` is not a schema fingerprint and has no stable serialized
form. Systems needing negotiated layout selection carry that information outside
the schema body. Any future extension must retain the one-tag/one-union-field
invariant unless it introduces a distinct, fully specified coordinated layout and
mutation model.

## 22. Initial scope, limitations, and roadmap

### 22.1 Initial scope

The initial release includes:

- logical records, closed scalar enums, and logical tagged enums with generated
  `*Ref`, `*Mut`, `*Patch`, and private `*Wire` support;
- fixed-width primitives, checked Boolean, bounded strings, fixed bytes, nonzero
  arrays, finite nesting, zero-sentinel optional all-zero-invalid schemas/enums,
  and mandatory externally tagged union fields;
- native/little/big scalar byte order and native-only borrowed wide strings;
- compact root, nested, array, union, and optional capabilities with short mutable
  field-local reborrows;
- field-named reads, field-named mutable handles, `set`, `copy_into`, and
  `copy_from` as the complete active movement vocabulary;
- generated `Default` patches and `From<Type> for TypePatch`;
- exact layout constants, metadata, and optional aligned receiving storage;
- eager safe access, `no_std`, fuzzing, Miri, and C++ layout coverage; and
- no handwritten unsafe implementation.

### 22.2 Explicit limitations

Generated C++ headers, open enums/unions, unknown payload preservation, shared
external tags, root-accessible tagged enums, dynamic sequences, zero-length
arrays, relative offsets, UTF-16 domain validation, packed records, atomic field
operations, schema fingerprints, automatic migration, raw union rewrites, and
range or checked-view array transfers are outside the initial promises.

### 22.3 Roadmap

Later work may add a packed representation backend, emitted C++ declarations,
open-union forwarding, or producer-owned transactional slot patterns. It must
retain the access boundary, private raw storage, type-preserving mutation,
allocation-free core, and no-handwritten-unsafe rule.

### 22.4 Unavoidable consequences

1. Safe access pays bounded eager type checking before it returns a capability;
   fixed arrays prove all `N` elements before an O(1)-state view exists.
2. Terminated-string methods can repeat a bounded terminator search because
   compact capabilities do not cache per-field facts.
3. The initial surface rejects `N = 0` fixed arrays.
4. C-compatible layout remains target- and compiler-condition-specific.
5. Native borrowed wide strings cannot directly represent foreign-endian units.
6. Cross-process mutation is outside Rust's normal reference guarantees.
7. A union switch requires a complete logical payload patch; partial patches can
   change only the current selected variant.
8. Fixed arrays provide exact whole-array logical movement, not ranges or
   arbitrary mutable wire elements.
9. Aggregate materialization explicitly pays construction cost while borrowed
   content remains tied to source bytes.
10. All successful access begins with an existing producer-supplied type-valid
    wire; the crate has no Rust-originated initialization route.

## 23. Full example (normative conceptual sketch)

**Normative conceptual sketch; not a standalone program.** The declarations and
fragments below specify the complete record, aggregate movement, mutation, and
tag-coupling contracts, while deliberately omitting a reviewed producer fixture,
application setup, and `main`. Use the [runnable application journeys](#runnable-application-journeys)
for executable producer-byte workflows.

```rust
use core::ffi::CStr;
use zero_schema::zero;

#[zero(endian = "native")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum ConfigKind {
    Memory = 1,
    File = 2,
}

#[zero(endian = "native")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header<'buf> {
    #[zero(capacity = 32)]
    pub producer: &'buf CStr,
}

#[zero(endian = "native")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryConfig {
    pub capacity_bytes: u64,
}

#[zero(endian = "native")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileConfig<'buf> {
    pub flags: u32,
    #[zero(capacity = 260)]
    pub path: &'buf CStr,
}

#[zero(endian = "native")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Config<'buf> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'buf>),
}

#[zero(endian = "native", align = 64)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message<'buf> {
    pub sequence: u64,
    pub samples: [u32; 3],
    pub header: Header<'buf>,
    #[zero(capacity = 64, len_type = u16)]
    pub name: &'buf str,
    pub config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    pub config: Config<'buf>,
}
```

Zero-copy inspection is direct and field-named:

```rust
fn inspect_existing(bytes: &[u8]) -> Result<(), MessageAccessError> {
    let message = Message::access(bytes)?;
    println!("sequence: {}", message.sequence());
    println!("producer: {:?}", message.header().producer());
    println!("name: {}", message.name());

    for sample in message.samples().iter() {
        println!("sample: {sample}");
    }

    let config = message.config();
    println!("kind: {:?}", config.tag());
    if let Some(file) = config.file() {
        println!("flags: {}", file.flags());
    }
    Ok(())
}
```

Materializing a complete logical record is explicit:

```rust
fn materialize_existing<'a>(bytes: &'a [u8]) -> Result<Message<'a>, MessageAccessError> {
    let message = Message::access(bytes)?;
    Ok(message.copy_into())
}
```

Mutation begins only from a valid producer wire and uses temporary
field-local capabilities:

```rust
fn mutate_existing(bytes: &mut [u8]) -> Result<(), MessageMutationError> {
    let mut message = Message::access_mut(bytes)?;

    message.sequence_mut().set(43)?;
    message.name_mut().set("replacement")?;
    message.header_mut().producer_mut().set(c"patched producer")?;

    let old_samples = message.samples().copy_into();
    message.samples_mut().copy_from(&old_samples)?;

    let mut config = message.config_mut();
    config.copy_from(&ConfigPatch::File(FileConfigPatch {
        flags: Some(0x20),
        path: Some(c"/tmp/input"),
    }))?;

    message.copy_from(&MessagePatch {
        sequence: Some(44),
        config_kind: Some(ConfigKind::File),
        config: Some(ConfigPatch::File(FileConfigPatch {
            flags: Some(0x40),
            path: None,
        })),
        ..MessagePatch::default()
    })
}
```

A source capability reaches a mutable destination only through a logical value
and patch:

```rust
fn transfer_complete(
    destination_bytes: &mut [u8],
    source_bytes: &[u8],
) -> Result<(), MessageMutationError> {
    let source = Message::access(source_bytes)?;
    let value = source.copy_into();
    let patch = MessagePatch::from(value);

    let mut destination = Message::access_mut(destination_bytes)?;
    destination.copy_from(&patch)
}
```

No operation in the example initializes arbitrary bytes. `Message::access_mut`
must succeed before a field mutation handle or `copy_from` is available. The
union's tag is read through `config().tag()` (and equivalently
`config_kind()`), never changed independently; union patch movement derives or
checks the coupled tag and writes it after its payload.

# Appendix A: attribute reference

## A.1 Container attributes

```rust
#[zero(
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    crate = path::to::zero_schema,
    borrow = 'lifetime,
)]
```

## A.2 Field attributes

```rust
#[zero(
    capacity = INTEGER,
    len_type = u8 | u16 | u32,
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    tag_field = sibling_identifier,
)]
```

| Attribute | Permitted use |
|---|---|
| `capacity` | borrowed string forms |
| `len_type` | `&str`, `&U16Str` |
| `endian` | direct numeric or length-prefixed storage |
| `align` | any supported field; the only field attribute allowed on zero-sentinel `Option<T>` |
| `tag_field` | required on every tagged-union record field; one matching unique sibling scalar enum |

For `Option<T>`, every listed field attribute other than `align` is a macro error;
the canonical Option spellings and inner acceptance/rejection matrix are in §6.5.

## A.3 Variant attributes

```rust
#[zero(tag = TagEnumType::Variant)]
```

Each tagged-enum variant must have exactly one matching declared scalar-enum
variant tag. Variant tag types must match the external sibling selected by each
containing union field.

## A.4 Defaults

| Item | Default |
|---|---|
| direct primitive endian | native |
| length form | `u16` |
| wire alignment | natural emitted alignment |
| field alignment | natural wire field alignment |
| scalar enum unknown value | error |
| external union unknown tag | error |
| type checking | eager |
| tagged-union tag placement | no default; an external `tag_field` is required |

# Appendix B: schematic macro expansion

For this input:

```rust
#[zero(endian = "native")]
#[derive(Clone, Debug, PartialEq)]
pub struct Example<'a> {
    pub id: u64,
    pub samples: [u32; 3],
    #[zero(capacity = 16, len_type = u8)]
    pub label: &'a str,
}
```

the macro retains `Example<'a>` and emits private storage, compact capabilities,
a patch, constants, metadata, and methods conceptually like this:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Example<'a> {
    pub id: u64,
    pub samples: [u32; 3],
    pub label: &'a str,
}

#[doc(hidden)]
#[repr(C)]
struct ExampleWire {
    id: U64NativeWire,
    samples: [U32NativeWire; 3],
    label: StrWire<U8Wire, 16>,
}

#[derive(Default)]
pub struct ExamplePatch<'a> {
    pub id: Option<u64>,
    pub samples: Option<[u32; 3]>,
    pub label: Option<&'a str>,
}

pub struct ExampleRef<'a> { /* checked private wire location */ }
pub struct ExampleMut<'a> { /* checked exclusive private wire location */ }

impl Example<'_> {
    pub const SCHEMA_SIZE: usize = core::mem::size_of::<ExampleWire>();
    pub const SCHEMA_ALIGN: usize = core::mem::align_of::<ExampleWire>();
    pub const SCHEMA_STRIDE: usize = round_up(Self::SCHEMA_SIZE, Self::SCHEMA_ALIGN);
    pub const LAYOUT: LayoutDescriptor = /* emitted static metadata */;

    pub fn access(bytes: &[u8]) -> Result<ExampleRef<'_>, ExampleAccessError> { /* emitted */ }
    pub fn access_mut(bytes: &mut [u8]) -> Result<ExampleMut<'_>, ExampleAccessError> { /* emitted */ }
}

impl<'a> ExampleRef<'a> {
    pub fn id(&self) -> u64 { /* emitted load */ }
    pub fn samples(&self) -> ArrayRef<'a, u32, 3> { /* checked location */ }
    pub fn label(&self) -> &'a str { /* checked bounded view */ }
    pub fn copy_into(&self) -> Example<'a> { /* explicit logical materialization */ }
}

impl<'a> From<Example<'a>> for ExamplePatch<'a> {
    fn from(value: Example<'a>) -> Self {
        Self {
            id: Some(value.id),
            samples: Some(value.samples),
            label: Some(value.label),
        }
    }
}

impl<'wire> ExampleMut<'wire> {
    pub fn id(&self) -> u64 { /* emitted load */ }
    pub fn id_mut(&mut self) -> ScalarMut<'_, u64> { /* checked field location */ }
    pub fn samples_mut(&mut self) -> ArrayMut<'_, u32, 3> { /* checked location */ }
    pub fn label_mut(&mut self) -> StringMut<'_> { /* checked bounded location */ }
    pub fn copy_into<'view>(&'view self) -> Example<'view> { /* materialize */ }
    pub fn copy_from(&mut self, patch: &ExamplePatch<'_>)
        -> Result<(), ExampleMutationError> { /* preflight all, then transfer */ }
}
```

`samples` physically maps to `[U32NativeWire; 3]`. `ArrayRef` and `ArrayMut`
remain the zero-copy representations; their complete aggregate operations are
`copy_into()` and exact-length `copy_from(&[u32])`. The actual emitted
implementation uses private support types rather than the ellipsis comments and
contains no handwritten `unsafe`.

# Appendix C: source references

The design relies on these external guarantees and APIs as of this document date:

1. [`zerocopy` crate documentation, version 0.8.54](https://docs.rs/zerocopy/0.8.54/zerocopy/)
2. [`zerocopy::FromBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.FromBytes.html)
3. [`zerocopy::TryFromBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.TryFromBytes.html)
4. [`zerocopy::IntoBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.IntoBytes.html)
5. [`zerocopy::KnownLayout`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.KnownLayout.html)
6. [`zerocopy::Immutable`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.Immutable.html)
7. [`zerocopy::Ref`](https://docs.rs/zerocopy/0.8.54/zerocopy/struct.Ref.html)
8. [`zerocopy::try_ref_from_bytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/fn.try_ref_from_bytes.html)
9. [`core::str::from_utf8`](https://doc.rust-lang.org/core/str/fn.from_utf8.html)
10. [`core::ffi::CStr`](https://doc.rust-lang.org/core/ffi/struct.CStr.html)
11. [`widestring` crate documentation, version 1.2.1](https://docs.rs/widestring/1.2.1/widestring/)
12. [`widestring::U16Str`](https://docs.rs/widestring/1.2.1/widestring/ustr/struct.U16Str.html)
13. [`widestring::U16CStr`](https://docs.rs/widestring/1.2.1/widestring/ucstr/struct.U16CStr.html)

# Final design summary

```text
#[zero] logical Type
    ordinary fields / variants and user derives
    access / access_mut
    SCHEMA_SIZE / SCHEMA_ALIGN / SCHEMA_STRIDE / LAYOUT
    │
    ├── TypeRef
    │     field() reads and nested / array / union capabilities
    │     copy_into() -> Type                  checked wire to logical value
    │
    ├── TypeMut
    │     field() shared-reborrow reads
    │     field_mut() -> temporary constrained field capability
    │     copy_into() -> Type                  current shared-reborrow result
    │     copy_from(&TypePatch)              logical patch into valid wire
    │
    ├── TypePatch
    │     Default = retain every field
    │     From<Type> = move every applicable field into a present entry
    │
    ├── ArrayRef / ArrayMut
    │     indexed zero-copy get / iter / get_mut
    │     complete copy_into / exact full-slice copy_from
    │
    ├── externally tagged union field
    │     one unique sibling scalar enum tag
    │     selected payload capability coordinates payload then tag
    │
    └── doc-hidden TypeWire
          repr(C), endian-aware scalar storage
          [T::Wire; N] arrays and union-sized payload storage
```

Safe access eagerly proves root size and alignment, constrained scalar encodings,
closed scalar enums, bounded strings, zero-sentinel option spans, external union
tags and selected payloads, and nested/array recursion. Ordinary parent padding,
unused capacity, and inactive payload bytes are ignored because they are not
logical fields.

Zero-copy field capabilities are the default path. `copy_into` explicitly copies
from checked wire into logical aggregates without allocation; borrowed content
retains its source lifetime. `copy_from` is the all-or-nothing logical patch or
full-array-slice operation from logical input into existing valid wire.
`From<Type> for TypePatch` only moves fields into present patch entries; it does
not perform a wire copy or add another mutation path.

Every tagged-union field has exactly one unique external scalar-enum sibling tag.
The field capability is the sole public coordinator for that tag and payload;
there is no standalone tagged-union root, embedded tag, raw union mutation, or
independent tag mutation. `access_mut` begins only with an existing type-valid
wire, and aligned receiving storage remains only storage for an external producer.
