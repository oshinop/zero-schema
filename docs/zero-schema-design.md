# zero-schema

## A Serde-like, fixed-layout, zero-copy schema system for Rust and C++ shared memory

**Status:** design proposal / implementation RFC  
**Document version:** 0.2  
**Date:** 2026-07-12  
**Reference implementation baseline:** `zerocopy` 0.8.54 and `widestring` 1.2.1

---

## Table of contents

1. [Abstract](#1-abstract)
2. [The problem](#2-the-problem)
3. [Goals and non-goals](#3-goals-and-non-goals)
4. [Core design](#4-core-design)
5. [Terminology and guarantees](#5-terminology-and-guarantees)
6. [Quick tour](#6-quick-tour)
7. [Complete syntax specification](#7-complete-syntax-specification)
8. [Wire-format semantics](#8-wire-format-semantics)
9. [Generated API and ergonomics](#9-generated-api-and-ergonomics)
10. [Parsing and validation](#10-parsing-and-validation)
11. [Encoding](#11-encoding)
12. [Tagged-union implementation](#12-tagged-union-implementation)
13. [Alignment and padding](#13-alignment-and-padding)
14. [Nested schemas and per-type composition](#14-nested-schemas-and-per-type-composition)
15. [Memory-safety argument](#15-memory-safety-argument)
16. [Shared-memory concurrency](#16-shared-memory-concurrency)
17. [Performance model](#17-performance-model)
18. [C++ interoperability](#18-c-interoperability)
19. [Errors and diagnostics](#19-errors-and-diagnostics)
20. [Implementation architecture](#20-implementation-architecture)
21. [Testing and verification](#21-testing-and-verification)
22. [Schema evolution](#22-schema-evolution)
23. [Initial feature set and roadmap](#23-initial-feature-set-and-roadmap)
24. [Known limitations and deliberate trade-offs](#24-known-limitations-and-deliberate-trade-offs)
25. [Full example](#25-full-example)
26. [Appendix A: attribute reference](#appendix-a-attribute-reference)
27. [Appendix B: schematic macro expansion](#appendix-b-schematic-macro-expansion)
28. [Appendix C: source references](#appendix-c-source-references)

---

## 1. Abstract

`zero-schema` is a proposed Rust framework for describing fixed-layout binary records that are shared with C or C++ code, especially through shared memory, memory-mapped files, DMA buffers, IPC slots, and other preallocated byte regions.

The framework combines four ideas:

1. **Serde-like per-type derives and helper attributes.** Schemas are ordinary Rust structs and enums annotated with `#[derive(ZeroSchema)]` and `#[zero(...)]`.
2. **Generated hidden wire types.** The public Rust type is ergonomic and validated; a separate generated `#[repr(C)]` type describes the actual bytes.
3. **`zerocopy` as the representation-safety layer.** Generated wire types derive `FromBytes`, `KnownLayout`, and `Immutable`, allowing safe zero-copy views over appropriately sized and aligned memory.
4. **Generated semantic validation and projection.** The framework validates relationships that `zerocopy` cannot express by itself, such as UTF-8 validity, NUL termination, length bounds, enum values, and tag-to-payload relationships.

The intended user experience is:

```rust
let message: Message<'_> = Message::parse(shared_memory_slot)?;

println!("{}", message.name);
println!("{}", message.path.display());

match message.config {
    Config::Memory(memory) => println!("{}", memory.capacity_bytes),
    Config::File(file) => println!("{}", file.path.display()),
}
```

The public value contains native Rust numbers, borrowed `&str`, `&CStr`, `&widestring::U16Str`, `&widestring::U16CStr`, nested schema values, and normal Rust enums. It does not expose `zerocopy::byteorder` wrappers, raw payload buffers, process-local pointers, or unsafe union access.

Encoding writes directly into the caller’s final fixed-size destination buffer without allocating an intermediate serialization buffer. Encoding necessarily writes and copies field contents; therefore this document calls it **direct-to-buffer encoding**, not literally zero-copy encoding.

---

## 2. The problem

### 2.1 Existing shared-memory formats are usually physical, not semantic

A typical C++ shared-memory record may look like this:

```cpp
struct FileConfig {
    std::uint32_t flags;
    char16_t path[260];
};

struct MemoryConfig {
    std::uint64_t capacity_bytes;
};

enum class ConfigKind : std::uint16_t {
    Memory = 1,
    File = 2,
};

union ConfigPayload {
    MemoryConfig memory;
    FileConfig file;
};

struct Message {
    std::uint64_t sequence;
    char name[64];
    ConfigKind kind;
    ConfigPayload config;
};
```

This representation has several useful properties:

- fixed maximum size;
- no heap allocation;
- direct placement in shared memory;
- well-known offsets;
- easy access from C++;
- predictable cache behavior.

It also has several hazards:

- the tag and active union member can disagree;
- string arrays may lack a terminator;
- enum storage may contain unknown values;
- padding and alignment depend on layout rules;
- direct pointer casting can be misaligned or violate object validity;
- a field may be physically readable but semantically invalid;
- concurrent writers can invalidate references held by readers.

### 2.2 A direct Rust mirror is not ergonomic or fully safe

A low-level Rust mirror might use raw integer fields, arrays, and a `union`:

```rust
#[repr(C)]
struct MessageWire {
    sequence: u64,
    name: [u8; 64],
    kind: u16,
    config: ConfigPayloadWire,
}
```

That preserves layout, but application code still has to perform all of the following correctly:

- find the first NUL in `name`;
- validate UTF-8 when a `&str` is desired;
- validate and map `kind` to a Rust enum;
- select the correct payload type;
- perform unsafe union access;
- validate nested strings inside the selected payload;
- handle endianness;
- avoid references to misaligned data;
- preserve lifetimes tied to the input memory;
- initialize padding and inactive union bytes during writes.

A conventional deserializer solves the semantic problem by constructing an owned Rust object, but that usually allocates or copies variable-size data and gives up direct shared-memory access.

### 2.3 `zerocopy` solves representation validity, not the whole schema

`zerocopy` can safely interpret initialized bytes as a type when size, alignment, layout, and bit-validity requirements are met. Its `FromBytes` trait means every bit pattern is valid for a type, while `TryFromBytes` adds runtime validity checks for types with invalid bit patterns. It also provides `KnownLayout`, `Immutable`, `Unaligned`, and byte-order-aware numeric wrappers.

However, the following invariants are relational or semantic rather than intrinsic bit validity:

```text
length <= capacity
bytes[..length] is valid UTF-8
there is a NUL within a fixed array
units before the NUL contain no earlier NUL
reserved bytes are zero
tag 2 means the payload is interpreted as FileConfig
offset + length does not overflow
version permits the selected variant
```

A derived `TryFromBytes` implementation for a struct cannot infer these application-level relationships from the field types alone.

### 2.4 The desired Rust model is stronger than the wire model

The desired application type is closer to:

```rust
pub struct Message<'a> {
    pub sequence: u64,
    pub name: &'a str,
    pub config: Config<'a>,
}

pub enum Config<'a> {
    Memory(MemoryConfig),
    File(FileConfig<'a>),
}

pub struct FileConfig<'a> {
    pub flags: u32,
    pub path: &'a widestring::U16CStr,
}
```

This type excludes many invalid states:

- `name` is known-valid UTF-8;
- `path` is known to contain a terminating NUL and no interior NUL before it;
- `config` cannot simultaneously be a `File` and a `Memory` payload;
- scalar enum fields contain only declared variants;
- references cannot outlive the input buffer.

The project must create this stronger model without allocating or copying the variable-size field contents.

### 2.5 Encoding has a different set of risks

Writing a logical value back to a fixed wire buffer must:

- enforce capacities;
- convert numbers to the declared byte order;
- NUL-terminate C strings;
- clear unused string capacity;
- clear inactive union bytes;
- write a tag that matches the payload;
- initialize all padding bytes deterministically;
- avoid exposing uninitialized memory;
- write directly to the final destination.

The aggregate generated wire type may contain padding or a Rust union and therefore may not safely derive `IntoBytes`. The encoder must not depend on transmuting an arbitrary Rust value into bytes.

---

## 3. Goals and non-goals

### 3.1 Goals

The initial project should provide:

- per-type `#[derive(ZeroSchema)]` ergonomics;
- direct public field access rather than getter-only generated views;
- zero-allocation, zero-copy decoding of borrowed string and payload data;
- fixed-size, caller-provided buffers;
- native Rust scalar values in the public model;
- `&str` and `&CStr` support;
- `&widestring::U16Str` and `&widestring::U16CStr` support;
- explicit endianness for numeric fields;
- ordinary fieldless enums represented by `u8`, `u16`, or `u32`;
- tagged unions exposed as ordinary Rust data-carrying enums;
- internally stored or externally stored union tags;
- nested schema structs and nested tagged unions;
- deterministic C-compatible field order, alignment, and padding;
- type-level and field-level custom alignment;
- direct-to-final-buffer encoding;
- generated structured errors;
- `no_std` operation for decoding and encoding, with optional `alloc` and `std` integrations;
- a small, auditable unsafe implementation boundary;
- C++ layout verification and eventual header generation.

### 3.2 Non-goals for the first release

The first release should not attempt to be:

- a replacement for Serde’s general-purpose data model;
- a variable-length object graph or arbitrary pointer serialization system;
- a transparent way to put Rust references, `String`, `Vec`, `Box`, or process-local pointers in shared memory;
- a mechanism that makes concurrently mutating shared memory safe;
- a promise that encoding performs no memory writes or copies;
- a portable way to expose non-native-endian UTF-16 directly as `&U16Str`;
- a general packed-struct framework;
- a schema-evolution system with automatic field-number compatibility like Protobuf or FlatBuffers;
- an automatic guarantee that independently written C++ declarations match without layout assertions.

---

## 4. Core design

The central design is a two-type model generated from one ordinary Rust declaration.

### 4.1 Public validated type

The user writes and uses this type:

```rust
#[derive(ZeroSchema)]
pub struct Message<'a> {
    pub sequence: u64,

    #[zero(capacity = 64)]
    pub name: &'a str,

    pub config: Config<'a>,
}
```

This is a real Rust type. It can be:

- pattern matched;
- directly field-accessed;
- constructed with a struct literal for encoding;
- combined with normal derives such as `Debug`, `Clone`, `Copy`, and `PartialEq` when its fields permit them;
- nested in other `ZeroSchema` types.

### 4.2 Hidden generated wire type

The derive emits a module-scope, doc-hidden support module and raw-identifier-derived public error names. For `Message`, the ABI-facing generated names are `MessageDecodeError` and `MessageEncodeError`; the hidden module and its `Wire` type are implementation details. Aligned storage is the runtime generic `AlignedBytes`, not a schema-named generated type. Generated wire aggregates derive `zerocopy::FromBytes`, `zerocopy::KnownLayout`, and `zerocopy::Immutable` at module scope.

These derives resolve the consuming crate's direct `zerocopy` dependency. A crate that derives `ZeroSchema` in 0.1 must therefore declare the exact supported `zerocopy` dependency with its `derive` feature; re-exporting it only through `zero_schema` is not the contract. Generated `#[zerocopy(crate = "...")]` annotations use the resolved direct-dependency name.

The wire type contains only representations for which arbitrary initialized bytes are valid:

- raw integer or byte-order wrapper storage;
- byte arrays;
- native `u16` arrays where required for borrowed `U16Str` views;
- nested generated wire types;
- generated union storage whose fields are themselves `FromBytes`.

It deliberately avoids placing these semantic types directly in the wire:

- Rust `bool`;
- fieldless Rust enums;
- `&str` or other references;
- `CStr` or `U16CStr` as inline values;
- the public data-carrying enum;
- values with cross-field validity invariants.

### 4.3 Decode pipeline

```text
&[u8]
  │
  ├─ check exact size and required alignment
  │
  ├─ zerocopy view as &GeneratedWire
  │
  ├─ decode native scalar values
  ├─ validate strings
  ├─ validate scalar enums
  ├─ select and validate tagged-union payload
  ├─ recursively decode nested schemas
  └─ construct the public borrowed schema value
```

### 4.4 Encode pipeline

```text
&PublicSchema
  │
  ├─ check destination size and alignment
  ├─ zero the final destination buffer
  ├─ validate capacities and custom invariants
  ├─ write scalars at generated offsets
  ├─ copy strings directly into final inline storage
  ├─ encode nested schemas directly in place
  ├─ encode selected union payload directly in place
  └─ write the matching tag
```

No intermediate serialized `Vec<u8>` is required.

---

## 5. Terminology and guarantees

### 5.1 Logical schema

The user-declared Rust struct or enum, such as `Message<'a>` or `Config<'a>`.

### 5.2 Wire type

The generated fixed-layout type describing bytes in shared memory. The wire type is not the public application model.

### 5.3 Representation validity

The bytes satisfy the requirements to form a Rust reference to the generated wire type:

- correct total size;
- correct starting alignment;
- initialized bytes;
- valid field layouts;
- valid intrinsic bit patterns.

`zero-schema` arranges for generated aggregate wire types to be `FromBytes`, so representation validity does not require scanning their contents.

### 5.4 Semantic validity

The wire fields satisfy the schema’s higher-level rules:

- valid lengths;
- valid UTF-8;
- NUL termination;
- valid enum values;
- valid union tag;
- valid selected payload;
- zero-required padding or tails;
- custom validation functions.

### 5.5 Zero-copy decoding

Variable-size contents such as strings and selected nested payloads remain in the source buffer. The public view stores references to those bytes or code units.

Small scalar values are loaded into native Rust values. Copying a `u32` into the public projection is not treated as object-graph materialization and is normally preferable to exposing `&u32` or endian wrappers.

### 5.6 Direct-to-buffer encoding

Encoding writes directly to the caller’s destination. It does not build an intermediate message buffer and then copy that buffer into shared memory.

It is not literally zero-copy: inline strings and other fields must be written to the destination.

### 5.7 Canonical encoding

An encoding is canonical when unused capacity, inactive union storage, and generated padding bytes are deterministically zeroed. `zero-schema` encoding is canonical by default.

### 5.8 Stable snapshot

An immutable period during which no other thread or process modifies the source bytes. A `zero-schema` decoded value is only sound while its source memory remains stable.

---

## 6. Quick tour

### 6.1 Schema declarations

```rust
use std::ffi::CStr;
use widestring::U16CStr;
use zero_schema::ZeroSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[repr(u8)]
#[zero(endian = "native")]
pub enum State {
    Initializing = 0,
    Ready = 1,
    Failed = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "native")]
pub enum ConfigKind {
    Memory = 1,
    File = 2,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct Header<'a> {
    pub version: u16,

    #[zero(capacity = 32, tail = "zero")]
    pub producer: &'a CStr,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct MemoryConfig {
    pub capacity_bytes: u64,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct FileConfig<'a> {
    pub flags: u32,

    #[zero(capacity = 260, tail = "zero")]
    pub path: &'a U16CStr,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(tag = ConfigKind)]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),

    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(
    endian = "native",
    align = 64,
    padding = "ignore"
)]
pub struct Message<'a> {
    pub sequence: u64,
    pub state: State,
    pub header: Header<'a>,

    #[zero(capacity = 64, len_type = u16, tail = "zero")]
    pub name: &'a str,

    // No tag_field attribute means the Config wire value contains its own tag.
    pub config: Config<'a>,
}
```

### 6.2 Parsing

```rust
let message = Message::parse(slot_bytes)?;

println!("sequence = {}", message.sequence);
println!("name = {}", message.name);
println!("producer = {:?}", message.header.producer);

match message.config {
    Config::Memory(memory) => {
        println!("capacity = {}", memory.capacity_bytes);
    }
    Config::File(file) => {
        println!("flags = {}", file.flags);
        println!("path = {}", file.path.display());
    }
}
```

All public field access is infallible because parsing eagerly established the field invariants.

### 6.3 Encoding

```rust
use widestring::u16cstr;

let message = Message {
    sequence: 42,
    state: State::Ready,
    header: Header {
        version: 3,
        producer: c"worker-service",
    },
    name: "active configuration",
    config: Config::File(FileConfig {
        flags: 0x03,
        path: u16cstr!(r"C:\data\cache.bin"),
    }),
};

let destination = message.encode()?;
```

For a monomorphic or lifetime-only schema, `encode()` returns owned `AlignedBytes`
whose address satisfies the wire alignment and whose byte view is exactly `WIRE_SIZE`.
Its return type is independent of the value's borrowed input lifetime.

---

## 7. Complete syntax specification

## 7.1 One derive for structs, scalar enums, and tagged enums

The primary API is:

```rust
#[derive(ZeroSchema)]
```

The derive accepts a module-scope item and inspects its shape:

- a named-field struct becomes a record schema;
- a fieldless enum with `#[repr(u8)]`, `#[repr(u16)]`, or `#[repr(u32)]` becomes a scalar enum schema;
- an enum with payload variants and `#[zero(tag = TagType)]` becomes a tagged-union schema.

The input must be a module-scope item whose visibility can be reproduced for generated sibling support items. Deriving on a function-local item is unsupported in 0.1. Raw identifiers are accepted; generated names use the identifier without its `r#` spelling. The grammar is the implemented `#[zero(...)]` grammar in this section: ordinary paths required by tag and validator options may not carry generic arguments. The derive preserves supported lifetimes, type parameters, const parameters, and where clauses, but does not promise a generated aligned buffer for schemas with type or const parameters.

The original Rust item remains in the program, and the derive emits wire types, trait implementations, inherent methods, concrete errors, and layout metadata. Because a derive cannot replace field types, schemas use valid ordinary Rust types such as `&'a str` and `&'a CStr`.

## 7.2 Struct syntax

```rust
#[derive(ZeroSchema)]
#[zero(
    endian = "native",
    align = 64,
    padding = "ignore",
    validate_with = validate_message
)]
pub struct Message<'a> {
    pub id: u64,

    #[zero(capacity = 64, len_type = u16, tail = "zero")]
    pub name: &'a str,

    pub header: Header<'a>,
}
```

### Container options for structs

| Option | Values | Default | Meaning |
|---|---|---:|---|
| `endian` | `"native"`, `"little"`, `"big"` | `"native"` | Default byte order for primitive fields declared directly in this struct |
| `align` | power-of-two integer | natural wire alignment | Raises alignment of the generated wire type |
| `padding` | `"ignore"`, `"zero"` | `"ignore"` | Decode policy for implicit struct padding; encoding always writes zero padding |
| `validate_with` | function path | none | Whole-value semantic validator called after built-in projection |
| `crate` | Rust path | resolved automatically | Override path to the `zero_schema` runtime crate |
| `borrow` | lifetime name | inferred | Select the input-buffer lifetime when multiple lifetimes exist |

`endian` does not recursively override a nested schema’s own declared representation. Each schema type has one stable wire representation. A newtype should be used when the same logical value must appear in two different wire endiannesses.

## 7.3 Supported public field types

### Primitive values

```rust
u8, i8,
u16, i16,
u32, i32,
u64, i64,
f32, f64,
bool
```

`u128`, `i128`, `usize`, `isize`, `char`, raw pointers, and function pointers are not part of the initial portable surface.

### Borrowed strings

```rust
&'a str
&'a std::ffi::CStr
&'a widestring::U16Str
&'a widestring::U16CStr
```

Each requires `#[zero(capacity = N)]`.

### Borrowed fixed bytes

```rust
&'a [u8; N]
```

This maps directly to `[u8; N]` in the wire and avoids copying the fixed byte block into the public view.

### Nested schemas

```rust
Header<'a>
Point
Config<'a>
```

Any otherwise unrecognized field type is treated as a nested schema and must implement the generated schema traits.

### Initially unsupported field shapes

```rust
String
Vec<T>
Box<T>
&T                  // except supported borrowed logical views
Option<T>
HashMap<K, V>
[T]                 // unsized dynamic slice
arbitrary tuples
raw pointers
```

Optional values should initially be modeled with an explicit tagged enum.

## 7.4 Field attributes

### Attribute validation policy

Helper attributes are part of the wire-format contract, not advisory hints. The derive must reject a `#[zero(...)]` option when any of the following is true:

- the option name is unknown;
- the option is attached to the wrong item category, such as a field-only option on a container;
- the option is not applicable to the field's logical type;
- a singleton option appears more than once across one or more `#[zero(...)]` attributes;
- two options are contradictory;
- the option value has the wrong syntactic form or is outside its permitted range;
- a referenced lifetime, variant, or sibling field cannot be resolved.

No recognized option may be silently ignored. Diagnostics must point at the offending option when possible and may add a secondary span for the field, variant, or declaration that makes the option invalid.

Field applicability in the initial release is:

| Option | Valid field categories | Additional rules |
|---|---|---|
| `capacity` | `&str`, `&CStr`, `&U16Str`, `&U16CStr` | Required exactly once for these fields; invalid on every other field category |
| `len_type` | `&str`, `&U16Str` | Must be `u8`, `u16`, or `u32`; the capacity must fit |
| `tail` | all four supported borrowed string categories | Controls bytes or code units after the logical end or first terminator |
| `endian` | directly declared numeric storage and length-prefixed string storage | Nested schemas and scalar enums retain their own representation; direct wide-string unit storage must remain native-endian |
| `align` | every supported field category | Must be a power of two accepted by Rust's representation rules |
| `tag_field` | a field implementing the generated tagged-union schema trait | Must name a sibling scalar-enum field whose type is exactly the union's associated tag type |
| `validate_with` | every projected logical field | The validator signature must type-check for the projected field type |
| `range` | directly declared primitive numeric fields | The range expression endpoints must type-check against the projected value |
| `must_equal` | directly declared scalar fields | The constant expression must type-check against the projected value |

Built-in logical types are recognized syntactically. A type alias that hides `&str`, `&CStr`, `&U16Str`, or `&U16CStr` is treated as an ordinary nested type rather than as built-in string syntax. Consequently, string-specific options on such an alias are rejected. Supporting aliases would require a different front end with access to resolved type information.

Some relationships can only be proved by Rust type checking. For example, a proc macro can see that `tag_field = kind` names a sibling, but it cannot inspect an arbitrary external type to discover its trait implementations during expansion. In those cases the derive emits a span-preserving trait or associated-type assertion, as specified in Section 20.3.

### `capacity`

Required for fixed-capacity logical strings:

```rust
#[zero(capacity = 64)]
pub name: &'a str,
```

For `str`, capacity counts bytes. For `U16Str`, capacity counts `u16` code units. For `CStr` and `U16CStr`, capacity includes room for the terminating zero.

### `len_type`

Selects the length-prefix representation for `str` and `U16Str`:

```rust
#[zero(capacity = 1024, len_type = u16)]
pub text: &'a str,
```

Supported values are `u8`, `u16`, and `u32`. The default is `u16`.

The derive rejects a capacity that cannot be represented by the chosen length type.

### `tail`

Controls decode validation of unused fixed-capacity storage:

```rust
#[zero(capacity = 64, tail = "ignore")]
#[zero(capacity = 64, tail = "zero")]
```

- `ignore`: unused bytes or code units may contain any value;
- `zero`: all storage after the logical value or first NUL must be zero.

Encoding always zeros the unused tail.

### `endian`

Overrides the struct default for a directly declared numeric or length-prefixed field:

```rust
#[zero(endian = "big")]
pub network_code: u32,
```

A nested schema or scalar-enum field uses that type’s own wire definition rather than a parent override.

### `align`

Raises the alignment of the field’s generated wire storage:

```rust
#[zero(align = 32)]
pub config: Config<'a>,
```

This has aligned-wrapper semantics: the field storage itself has alignment `N`, and its size is rounded up as required by that alignment.

### `tag_field`

Uses a sibling scalar-enum field as the physical tag for a tagged-union payload:

```rust
pub kind: ConfigKind,

#[zero(tag_field = kind)]
pub config: Config<'a>,
```

Without this attribute, the union's generated wire type stores its own tag.

The attribute is valid only on a field whose type implements the generated tagged-union schema trait. It is a compile-time error to place it on `str`, a primitive, a scalar enum, an ordinary nested struct, or any other non-union field. The named target must exist in the same struct, must be a scalar `ZeroSchema` enum, and must have exactly the union's declared tag type.

For example, both of these are invalid:

```rust,compile_fail
#[zero(tag_field = kind)]
pub name: &'a str, // `name` is not a tagged union

pub kind: &'a str, // `kind` is not a scalar tag enum
#[zero(tag_field = kind)]
pub config: Config<'a>,
```

### `validate_with`

Runs a standardized custom field validator after built-in conversion:

```rust
#[zero(validate_with = validate_name)]
pub name: &'a str,
```

The initial standardized signature is:

```rust
fn validate_name(
    value: &str,
    context: &zero_schema::ValidationContext<'_>,
) -> zero_schema::ValidationResult;
```

### `range` and `must_equal`

Optional declarative validation sugar:

```rust
#[zero(range = 0..=100)]
pub percentage: u8,

#[zero(must_equal = 0)]
pub reserved: u32,
```

These are convenience features layered on the same generated validation framework.

## 7.5 Scalar enum syntax

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "little")]
pub enum Mode {
    Disabled = 0,
    Memory = 1,
    File = 2,
}
```

Rules:

- the enum must be fieldless;
- representation must be `u8`, `u16`, or `u32` in the initial release;
- every variant must have an explicit discriminant;
- discriminants must be unique and fit the representation;
- unknown wire values are decode errors;
- the generated wire field is a raw integer representation, not the Rust enum itself.

The raw-integer wire representation avoids creating an invalid Rust enum from arbitrary C++ or shared-memory bytes.

## 7.6 Tagged-union syntax

```rust
#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(tag = ConfigKind)]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),

    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
}
```

Initial variant shapes:

- one newtype payload field containing another schema; or
- a unit variant.

Named multi-field variants can be supported later by generating a synthetic payload schema, but the initial syntax keeps layout and error reporting simple.

The tag type must be a scalar `ZeroSchema` enum.

## 7.7 Internal tag versus external tag

### Internal tag

```rust
pub config: Config<'a>,
```

Generated wire shape:

```text
ConfigWire {
    tag,
    payload_union,
}
```

The public type contains only the Rust enum because the variant already communicates the tag.

### External sibling tag

```rust
pub kind: ConfigKind,

#[zero(tag_field = kind)]
pub config: Config<'a>,
```

Generated wire shape:

```text
MessageWire {
    kind,
    config_payload_union,
}
```

The external-tag relationship is well formed only when all of the following hold:

1. the field carrying `tag_field` implements the generated tagged-union schema trait;
2. the identifier names a field in the same struct;
3. the target field implements the scalar-enum schema trait;
4. the target field's Rust type is exactly the tagged union's associated `Tag` type.

Declaration order does not matter because the derive resolves references after collecting all fields. A missing sibling is diagnosed at the `tag_field` value. A non-union payload, non-scalar target, or associated-tag type mismatch is diagnosed through a direct macro error when syntax is sufficient and otherwise through a generated span-preserving trait assertion.

The public value retains both declared fields. Parsing guarantees they agree. Encoding rejects a public value whose `kind` does not equal `config.tag()`.

Internal tagging is recommended for new schemas because it cannot represent a redundant inconsistent public state.

## 7.8 Lifetimes

A schema with borrowed fields has one designated input-buffer lifetime:

```rust
pub struct Message<'a> {
    pub name: &'a str,
}
```

Rules for 0.1:

- zero or one lifetime parameter is inferred without annotation;
- with multiple lifetimes, `#[zero(borrow = 'buf)]` must select the source-buffer lifetime and the declared lifetime bounds must make every borrowed field compatible with it;
- type and const generics and where clauses are preserved and participate in generated trait/layout bounds;
- generated references are tied to the selected source lifetime;
- the wire contains no reference or lifetime state;
- function-local derives are unsupported; zero-argument `encode()` is emitted only when the schema has no type or const parameters, while `make_buffer_for!(FullyConcreteType)` works after any required monomorphization.

## 7.9 Custom type alignment

```rust
#[derive(ZeroSchema)]
#[zero(align = 64)]
pub struct CacheLineRecord<'a> {
    // ...
}
```

The generated wire type uses `#[repr(C, align(64))]`. The public projection itself is not forced to have that alignment.

## 7.10 Representation defaults

Unless overridden:

```text
wire struct order     C field order
wire union layout     C union size/alignment rules
endianness            native
struct padding decode ignored
struct padding encode zero
string tail decode    ignored
string tail encode    zero
scalar enum unknown   error
union unknown tag     error
validation            eager
```

---

## 8. Wire-format semantics

## 8.1 Primitive numeric fields

A public field:

```rust
pub value: u32,
```

maps to a hidden endian-aware storage wrapper. Conceptually, a little-endian wrapper preserving the target’s native `u32` alignment can be generated as:

```rust
#[repr(C)]
struct __U32Le {
    // A zero-length array contributes u32 alignment without occupying bytes.
    _align: [u32; 0],
    value: zerocopy::byteorder::U32<zerocopy::byteorder::LittleEndian>,
}
```

Rust guarantees that `[T; 0]` has size zero and the alignment of `T`. The wrapper therefore has the expected scalar alignment while retaining an explicit byte order.

The public projection receives a native `u32` through `value.get()`.

For native-endian fields, the implementation may use either a native primitive or the corresponding `NativeEndian` wrapper. The generated wire layout, not this implementation choice, is the compatibility contract.

## 8.2 Boolean fields

A public `bool` is stored as `u8`:

```text
0 => false
1 => true
other => decode error
```

The wire does not contain a Rust `bool`, so arbitrary input bytes never create an invalid `bool` reference.

## 8.3 `&str`

Declaration:

```rust
#[zero(capacity = 64, len_type = u16)]
pub name: &'a str,
```

Conceptual wire:

```rust
#[repr(C)]
struct __NameWire {
    len: __U16Wire,
    bytes: [u8; 64],
}
```

Decode checks:

1. `len <= 64`;
2. `bytes[..len]` is valid UTF-8;
3. if `tail = "zero"`, `bytes[len..]` is all zero.

Public projection:

```rust
&'a str
```

Encoding checks capacity, writes the length, copies the UTF-8 bytes, and leaves the remaining zeroed storage untouched.

Length-prefixed `str` may contain interior NUL bytes because Rust `str` permits them.

## 8.4 `&CStr`

Declaration:

```rust
#[zero(capacity = 64)]
pub executable: &'a CStr,
```

Wire:

```rust
[u8; 64]
```

Decode uses the first NUL as the terminator. The scan never goes beyond the fixed capacity. No UTF-8 validation is performed because `CStr` may contain non-UTF-8 bytes.

Public projection:

```rust
&'a CStr
```

Applications can explicitly request UTF-8 later with `CStr::to_str()`.

Capacity includes the terminator, so the maximum content length is `capacity - 1`.

## 8.5 `&U16Str`

Declaration:

```rust
#[zero(capacity = 128, len_type = u16)]
pub title: &'a widestring::U16Str,
```

Conceptual wire:

```rust
#[repr(C)]
struct __TitleWire {
    len: __U16Wire,
    units: [u16; 128],
}
```

Decode checks only the length bound and optional tail policy. `U16Str` intentionally does not require valid UTF-16; it is a borrowed sequence of 16-bit wide code units intended for FFI-style data.

The length counts code units, not Unicode scalar values and not bytes.

## 8.6 `&U16CStr`

Declaration:

```rust
#[zero(capacity = 260)]
pub path: &'a widestring::U16CStr,
```

Wire:

```rust
[u16; 260]
```

Decode scans for the first zero code unit and constructs a borrowed `U16CStr` ending at that terminator. `U16CStr` guarantees termination and no interior NUL before the terminator, but does not guarantee valid UTF-16.

Capacity includes the terminator.

## 8.7 Wide-string endianness constraint

A `&U16Str` or `&U16CStr` is a view over native `u16` objects. A buffer containing fixed little-endian code units is not safely viewable as `&[u16]` on a big-endian target without conversion.

Therefore the initial rule is:

> Direct borrowed `U16Str` and `U16CStr` fields require native-endian `u16` storage.

The macro rejects a direct wide-string field whose declared wire endianness does not match the compilation target.

A future portable type such as `EndianU16Str<'a, LittleEndian>` may provide native-value iteration without promising `&U16Str`.

## 8.8 Borrowed fixed byte arrays

Declaration:

```rust
pub digest: &'a [u8; 32],
```

Wire:

```rust
[u8; 32]
```

Projection returns a reference directly into the source buffer. Encoding copies exactly 32 bytes into the final destination.

## 8.9 Nested schemas

Declaration:

```rust
pub header: Header<'a>,
```

Wire field:

```rust
<Header<'a> as ZeroSchemaType>::Wire
```

Decode recursively projects `Header<'a>`. Encode recursively writes into the nested field’s byte range.

The parent macro does not inspect the source of `Header`; composition is trait-based.

## 8.10 Scalar enum fields

A public scalar enum is stored as its raw integer representation in the enum’s declared byte order.

Decode performs a generated `match`:

```rust
match raw {
    0 => Ok(State::Initializing),
    1 => Ok(State::Ready),
    2 => Ok(State::Failed),
    other => Err(StateDecodeError::UnknownValue(other)),
}
```

The wire never contains a Rust enum value, avoiding invalid-discriminant undefined behavior.

## 8.11 Tagged unions

The generated payload storage is a C-layout union whose fields are the generated wire types of every variant payload.

```rust
#[repr(C)]
#[derive(zerocopy::FromBytes, zerocopy::KnownLayout, zerocopy::Immutable)]
union __ConfigPayloadWire<'a> {
    memory: core::mem::ManuallyDrop<
        <MemoryConfig as ZeroSchemaType>::Wire,
    >,
    file: core::mem::ManuallyDrop<
        <FileConfig<'a> as ZeroSchemaType>::Wire,
    >,
}
```

The union exists to let the compiler compute the maximum payload size and alignment exactly. Public code never accesses it.

The generated decoder reads the tag, obtains a byte slice covering the payload storage, and views the selected prefix as the selected variant’s all-bit-valid wire type. It then performs that variant’s semantic projection.

## 8.12 Padding

Generated wire structs use C field order. The compiler may insert inter-field and trailing padding.

On decode:

- `padding = "ignore"` accepts any initialized padding bytes;
- `padding = "zero"` checks generated padding ranges for zero.

On encode:

- the complete destination is zeroed before fields are written;
- all implicit padding therefore remains initialized and canonical.

The framework does not derive `IntoBytes` for arbitrary padded aggregate wire types merely to serialize them. It writes fields directly at generated offsets.

---

## 9. Generated API and ergonomics

## 9.1 Inherent parsing methods

For a borrowed schema:

```rust
impl<'a> Message<'a> {
    pub fn parse(bytes: &'a [u8])
        -> Result<Self, MessageDecodeError>;

    pub fn parse_prefix(bytes: &'a [u8])
        -> Result<(Self, &'a [u8]), MessageDecodeError>;
}
```

`parse` requires an exact wire-size slice. `parse_prefix` parses one record from the leading bytes and returns the remainder.

## 9.2 Encoding methods

```rust
impl Message<'_> {
    pub fn encode(
        &self,
    ) -> Result<zero_schema::AlignedBytes<EncodedAlignment, { ENCODED_SIZE }>, MessageEncodeError>;

    pub fn encode_into(
        &self,
        destination: &mut [u8],
    ) -> Result<(), MessageEncodeError>;
}
```

The encoding length is the type's `WIRE_SIZE`; 0.1 does not generate an `encoded_len` method.

## 9.3 Layout constants

```rust
impl Message<'_> {
    pub const WIRE_SIZE: usize;
    pub const WIRE_ALIGN: usize;
    pub const WIRE_STRIDE: usize;
    pub const LAYOUT: &'static zero_schema::LayoutDescriptor;
}
```

`WIRE_STRIDE` is `WIRE_SIZE` rounded up to `WIRE_ALIGN` and is useful for arrays of shared-memory slots.

## 9.4 Owned aligned storage

`AlignedBytes<W, N>` is a public runtime type containing initialized owned bytes. A
zero-length `[W; 0]` imposes alignment without storing or initializing a `W`; its
byte view starts at offset zero and has length `N`, while trailing padding makes the
value size a multiple of `align_of::<W>()`. It provides `zeroed`, `as_bytes`,
`as_bytes_mut`, `AsRef<[u8]>`, and `AsMut<[u8]>`.

For schemas without type or const parameters—including lifetime-only schemas—
`encode()` returns the correctly specialized `AlignedBytes`. Type- or const-generic
schemas do not expose zero-argument `encode()`: stable Rust cannot express the
dependent array length in that generic inherent return type. After fully
monomorphizing such a schema, construct storage explicitly:

```rust
let mut bytes = zero_schema::make_buffer_for!(Envelope<'static, Concrete, 4>);
value.encode_into(bytes.as_bytes_mut())?;
```

`make_buffer_for!` accepts any fully concrete schema type and expands to
`AlignedBytes::<Schema::Wire, { Schema::WIRE_SIZE }>::zeroed()`. A plain `Vec<u8>`
does not guarantee arbitrary wire alignment.

## 9.5 Direct field access

The parsed result is the original public struct:

```rust
message.sequence
message.header.version
message.name
message.config
```

There are no generated getters for ordinary public fields.

## 9.6 Construction for encoding

Because the public type is ordinary Rust, values are built normally:

```rust
let file = FileConfig {
    flags: 7,
    path: widestring::u16cstr!(r"C:\cache"),
};

let message = Message {
    sequence: 1,
    state: State::Ready,
    header,
    name: "worker",
    config: Config::File(file),
};
```

The same type serves as both a decoded borrowed view and an encoding input.

---

## 10. Parsing and validation

## 10.1 Stage 1: exact size and alignment

The generated `parse` method invokes the generated wire type’s `zerocopy::FromBytes::ref_from_bytes` operation.

This checks:

- the source length equals the generated wire size;
- the source address satisfies the generated wire alignment.

Because the wire is `FromBytes`, no content scan is needed to create the root wire reference.

## 10.2 Stage 2: scalar projection

Endian-aware hidden numeric wrappers return native scalar values through `get()` or equivalent generated logic.

Boolean storage is explicitly checked for `0` or `1`.

## 10.3 Stage 3: string projection

- `str`: length bound and UTF-8 validation;
- `CStr`: bounded first-NUL scan;
- `U16Str`: length bound and borrowed slice construction;
- `U16CStr`: bounded first-zero scan and borrowed wide C string construction.

The resulting references point into the original source bytes.

## 10.4 Stage 4: scalar-enum validation

Raw integer values are matched to declared variants. No reference to a Rust enum is created until the value is known.

## 10.5 Stage 5: tagged-union selection

The tag is decoded first. Exactly one payload wire type is selected. Only the selected variant is semantically decoded.

Unknown tags produce an error in the initial closed-union model.

## 10.6 Stage 6: nested validation

Nested schema decoders recursively perform their own semantic projection. Errors are wrapped with the parent field and union-variant path.

## 10.7 Stage 7: custom validation

Field-level validators run after their field has been projected. Whole-value validators run after the public struct or enum has been constructed.

A whole-value validator can express relationships such as:

```rust
fn validate_message(
    value: &Message<'_>,
    _: &ValidationContext<'_>,
) -> ValidationResult {
    if value.state == State::Ready && value.header.version == 0 {
        return Err(ValidationFailure::new(
            1001,
            "ready messages require a nonzero version",
        ));
    }
    Ok(())
}
```

## 10.8 Eager validation is required for direct fields

The public type stores `&str`, `&CStr`, `&U16CStr`, and a selected Rust enum directly. Constructing it therefore requires validating those fields eagerly.

A separate future lazy API could return a raw-backed view with fallible accessors, but that is intentionally not the primary direct-field interface.

---

## 11. Encoding

## 11.1 Encoding is direct, not a transmute

The encoder does not construct a public wire value and call `as_bytes()` on it. That approach is fragile for:

- aggregate padding;
- generated unions;
- inactive union members;
- uninitialized tails;
- types that cannot derive `IntoBytes`.

Instead, the encoder treats the destination as initialized byte storage and writes each field to its generated byte range.

## 11.2 Encoding algorithm

For every top-level `encode_into`:

1. Check `destination.len() == WIRE_SIZE`.
2. Check `destination.as_ptr()` satisfies `WIRE_ALIGN`.
3. Run complete semantic validation in the specified precedence order.
4. Fill the complete destination with zero exactly once.
5. Write scalar fields in their declared byte order.
6. Write strings directly into fixed inline storage.
7. Recursively encode nested schemas into their final offsets.
8. Encode the selected union payload into the union storage.
9. Write the union tag that corresponds to the selected public variant.

The implementation may write the tag after the payload, but this ordering is not a shared-memory publication protocol by itself.

### Destination state on error

`encode_into` has a preflight implementation but intentionally retains a weaker
public contract. Size or alignment failure leaves the destination untouched, and
semantic validation currently completes before mutation. After layout checks pass,
however, any returned `Err` invalidates the destination: future write-phase errors
may leave it zeroed or partially written. The caller must not parse or publish it.

`encode()` keeps the destination private until success and drops it on error. For
caller-provided storage, applications requiring transactional publication must use
an exclusively owned inactive slot—typically a separate `make_buffer_for!` value—and
publish it only after `Ok(())`.

## 11.3 Generated offsets

Generated code uses compiler-computed layout constants such as:

```rust
core::mem::offset_of!(__MessageWire, sequence)
core::mem::offset_of!(__MessageWire, name)
core::mem::offset_of!(__MessageWire, config)
```

and `size_of`/`align_of` for nested wire types.

This lets per-type derives compose without the proc macro evaluating the final layout of arbitrary associated wire types itself.

## 11.4 Scalar writes

A scalar is converted to bytes and copied into its exact field range:

```rust
let encoded = value.to_le_bytes();
destination[offset..offset + 4].copy_from_slice(&encoded);
```

The generated implementation may instead use `zerocopy` byte-order values internally. No unaligned scalar reference is required.

## 11.5 String writes

### `str`

- verify `value.len() <= capacity`;
- encode length;
- copy UTF-8 bytes;
- unused bytes remain zero.

### `CStr`

- obtain bytes including NUL;
- verify the total including NUL fits capacity;
- copy them;
- unused bytes remain zero.

### `U16Str`

- verify code-unit length fits capacity and length type;
- write native-endian code units;
- unused units remain zero.

### `U16CStr`

- obtain code units including NUL;
- verify they fit capacity;
- write native-endian units;
- unused units remain zero.

## 11.6 Nested writes

The parent passes the exact nested field sub-slice to the nested schema’s generated encoder. No intermediate nested buffer is allocated.

## 11.7 Union writes

The payload storage is already zeroed. The encoder selects one variant, invokes that payload schema’s encoder on the prefix of the union storage, and writes the corresponding tag.

Inactive union bytes remain zero.

## 11.8 External tag consistency

For:

```rust
pub kind: ConfigKind,

#[zero(tag_field = kind)]
pub config: Config<'a>,
```

encoding checks:

```rust
self.kind == self.config.tag()
```

A mismatch returns `TagMismatch` rather than producing an inconsistent wire value.

## 11.9 Optional in-place writer API

A transactional or staged writer is not implemented in 0.1 and is explicitly outside the core contract. The only encoding guarantee is `encode_into`'s nontransactional contract in Section 11.2.

---

## 12. Tagged-union implementation

## 12.1 Why not put the public Rust enum directly in shared memory?

A data-carrying Rust enum can have a defined representation, but using it directly as the wire type creates several problems:

- arbitrary input bytes may contain an invalid discriminant;
- representation details are harder to mirror in existing C++ code;
- endian conversion of a multi-byte tag is awkward;
- variant padding and inactive storage become part of the Rust enum’s validity model;
- format evolution becomes coupled to compiler representation rules;
- public payload fields may contain borrowed strings that cannot exist inline.

`zero-schema` instead generates an explicit tag plus payload storage.

## 12.2 Why a generated C-layout union is still useful

Rust union field access is unsafe, but a hidden generated union gives the compiler exactly the calculation needed for C interoperability:

```text
payload size  = maximum variant wire size, rounded to payload alignment
payload align = maximum variant wire alignment
all variants begin at payload offset 0
```

These are the same core layout rules as a C union.

The public API never exposes this union.

## 12.3 Avoiding active-member assumptions

The decoder does not rely on an active-member model and does not create an ad-hoc byte slice from a union reference. After the tag has selected a variant, generated code accesses that union member in one audited unsafe expression and passes a shared reference to its `ManuallyDrop<VariantWire>` storage into the variant decoder.

The safety proof is local: the complete union is initialized as part of a `FromBytes` root wire; every member begins at offset zero; the union's size and alignment dominate each member; each member wire is `FromBytes + KnownLayout + Immutable + 'static`; and the returned borrow is bounded by the root input lifetime. No public API exposes the union or permits arbitrary union-byte access.

## 12.4 Internal tagged wire

Conceptually:

```rust
#[repr(C)]
struct __ConfigWire<'a> {
    tag: <ConfigKind as ZeroSchemaType>::Wire,
    payload: __ConfigPayloadWire<'a>,
}
```

## 12.5 External tagged payload

When a parent uses `#[zero(tag_field = kind)]`, its hidden wire field for `config` is only:

```rust
__ConfigPayloadWire<'a>
```

The parent decoder supplies the already decoded `kind` to the union decoder.

## 12.6 Unit variants

A unit variant has no logical payload. In 0.1 its generated payload member still uses non-zero-sized initialized storage, and every generated root wire must have nonzero size. Zero-sized root schemas, zero-sized nested wire fields, and zero-sized tagged payload members are rejected or fail generated compile-time layout assertions. The selected variant still requires a valid tag.

## 12.7 Unknown tags

The 0.1 design is closed: an unknown scalar value or tagged-union tag is a decode error. Unknown-value preservation is outside scope as specified once in Section 23.2.
---

## 13. Alignment and padding

## 13.1 Type alignment

```rust
#[zero(align = 64)]
pub struct Message<'a> { /* ... */ }
```

The generated wire type uses:

```rust
#[repr(C, align(64))]
```

The requested alignment must be a power of two permitted by Rust’s representation rules. Raising alignment may add trailing padding and increase slot stride.

## 13.2 Field alignment

```rust
#[zero(align = 32)]
pub block: Block,
```

The generated field is wrapped:

```rust
#[repr(C, align(32))]
struct __AlignedBlock<T> {
    value: T,
}
```

Consequences:

- the field starts at an offset satisfying alignment 32;
- the wrapper’s size is rounded up to its alignment;
- the following field starts after the rounded wrapper size.

This is stronger than merely inserting pre-field padding.

## 13.3 Nested alignment

A nested generated wire type’s alignment naturally propagates into its parent’s `repr(C)` layout. The parent derive does not need to know the numeric value; the Rust compiler computes it from the associated wire type.

## 13.4 Union alignment

A generated `repr(C)` union automatically has the maximum alignment required by any variant wire type. This avoids asking the proc macro to evaluate arbitrary nested layouts.

## 13.5 Runtime alignment checks

`Message::parse` checks the actual starting pointer alignment through `zerocopy`.

For a slot array:

```rust
let offset = index
    .checked_mul(Message::WIRE_STRIDE)
    .ok_or(Error::OffsetOverflow)?;

let slot = &mapping[offset..offset + Message::WIRE_SIZE];
let message = Message::parse(slot)?;
```

The mapping base and every slot offset must satisfy `WIRE_ALIGN`.

## 13.6 Padding validation

When `padding = "zero"`, generated code computes each direct padding range from compiler constants:

```text
end of previous field .. start of next field
end of final field .. size_of::<Wire>()
```

It rejects nonzero bytes in those ranges.

Nested schemas validate their own padding according to their own policies.

## 13.7 Why encoding starts with a full zero fill

The initial zero fill guarantees:

- all implicit padding is initialized;
- all fixed-string tails are canonical;
- all inactive union bytes are canonical;
- no previous slot contents leak;
- no uninitialized process memory is exposed.

It also avoids relying on aggregate `IntoBytes` for padded or union-containing types.

## 13.8 Packed layouts

Packed layouts are not part of the initial feature set. Packed fields can be misaligned, and Rust does not permit references to misaligned fields. That conflicts directly with returning borrowed `&U16Str`, `&U16CStr`, and nested wire references.

A future packed backend would need load/store-by-offset views rather than the same reference-based design.

---

## 14. Nested schemas and per-type composition

## 14.1 Why a module-level macro is not required

Each type derive emits:

- an associated hidden wire type;
- decode and encode trait implementations;
- layout constants;
- tagged-union traits when applicable.

A parent only needs those traits. It does not need to inspect the child’s source declaration.

## 14.2 Core composition traits

The implemented runtime contract is:

```rust
pub trait ZeroSchemaType: Sized {
    type Wire: zerocopy::FromBytes
        + zerocopy::KnownLayout
        + zerocopy::Immutable
        + 'static;
    type DecodeError: SchemaError + 'static;
    type EncodeError: SchemaError + 'static;

    const WIRE_SIZE: usize;
    const WIRE_ALIGN: usize;
    const WIRE_STRIDE: usize;
    const LAYOUT: &'static LayoutDescriptor;
}

#[doc(hidden)]
pub trait DecodeWire<'src>: ZeroSchemaType {
    fn decode_at(
        input: DecodeInput<'src, Self::Wire>,
    ) -> Result<Self, Self::DecodeError>;
}

#[doc(hidden)]
pub trait EncodeWire: ZeroSchemaType {
    fn validate_encode(&self) -> Result<(), Self::EncodeError>;
    fn encode_at(
        &self,
        destination: &mut Prezeroed<'_>,
    ) -> Result<(), Self::EncodeError>;
}
```

`DecodeInput` retains both the typed wire reference and its exact originating bytes so padding and nested ranges can be validated without treating an aggregate as bytes. `Prezeroed` is internal capability-typed destination storage: the root constructor fills exactly once, nested `subrange` operations never refill, and writes are bounds checked. These traits are cross-crate composition surfaces but `DecodeWire`, `EncodeWire`, and `Prezeroed` are doc-hidden implementation APIs, not user extension promises.

## 14.3 Borrowed child wire types

A generated child wire type contains no references even if its public schema has lifetimes. Those lifetimes are erased from `ZeroSchemaType::Wire`; decode implementations add a fresh source lifetime and only the generated `DecodeWire`/`DecodeTaggedUnion` bounds express the selected public borrow lifetime and its required shortening chain. Encode composition is available independently of any input lifetime.

## 14.4 Declaration order

Types may be declared before or after their users. Rust name resolution and trait checking occur after macro expansion.

## 14.5 Inline recursion

This is invalid because it has infinite size:

```rust
pub struct Node<'a> {
    pub child: Node<'a>,
}
```

Relative offsets, arenas, and recursive graphs are a separate future feature. The initial design supports finite inline nesting only.

---

## 15. Memory-safety argument

This section states the safety case the implementation must uphold.

## 15.1 Root wire references are created only by `zerocopy`

The generated aggregate wire type derives `FromBytes`, `KnownLayout`, and `Immutable` using `zerocopy`’s derives rather than manual unsafe trait implementations.

`FromBytes::ref_from_bytes` checks size and alignment before returning a reference.

## 15.2 Generated wire types are all-bit-valid

The generator avoids semantic types with invalid bit patterns:

| Public type | Wire storage |
|---|---|
| `bool` | `u8` |
| fieldless enum | raw integer wrapper |
| `&str` | length wrapper plus `[u8; N]` |
| `&CStr` | `[u8; N]` |
| `&U16Str` | length wrapper plus `[u16; N]` |
| `&U16CStr` | `[u16; N]` |
| tagged enum | raw tag plus all-bit-valid payload union |

No invalid semantic reference is formed until validation succeeds.

## 15.3 String references use safe constructors

- `str` uses UTF-8 validation over a bounds-checked slice;
- `CStr` uses a safe bounded slice constructor;
- `U16Str` uses `U16Str::from_slice`, which permits arbitrary code units;
- `U16CStr` uses a safe bounded first-NUL constructor.

The returned lifetime comes from the wire reference, which comes from the input byte slice.

## 15.4 Scalar enums are constructed by matching raw values

The implementation never reinterprets arbitrary bytes as the public Rust enum. It matches a raw integer and constructs a declared variant only on success.

## 15.5 Tagged payload access remains safe and byte-origin preserving

The publishable runtime, derive crate, and generated tokens contain no handwritten `unsafe`. Tagged decoding never reads a Rust union field and never forms a byte slice from aggregate or union storage. `DecodeInput` carries the exact original input bytes beside the typed root view; checked `subrange` construction selects the payload range, verifies its bounds and alignment, and uses `zerocopy` to form only the selected all-bit-valid payload view.

Soundness follows from these enforced invariants:

1. the original input range covers the complete root wire;
2. checked offset addition cannot wrap and the selected range is in bounds;
3. the selected payload start satisfies its wire alignment;
4. the selected payload wire is `FromBytes + KnownLayout + Immutable`;
5. inactive payload bytes are inspected through the original byte range rather than a typed aggregate byte cast;
6. every returned reference is bounded by the source lifetime carried by `DecodeInput`.

Generated const assertions establish payload member size, alignment, and offsets. Unpublished FFI and counting-allocator test support maintain their own explicit unsafe inventories and safety proofs; they are outside the publishable codec path.

## 15.6 Encoding does not create invalid mutable typed references

The encoder works on `&mut [u8]` and generated field ranges. It does not cast a byte slice to `&mut AggregateWire` unless the aggregate satisfies all required `IntoBytes` conditions.

This avoids the subtle requirements around mutating padded structs and writing unions.

## 15.7 All writes are bounds checked

Generated offsets and sizes are compiler constants. Before slicing or copying, the encoder checks the root destination size. Internal range calculations use checked arithmetic in generic runtime helpers.

## 15.8 The source must remain immutable

A decoded public value contains references into the input. Mutating those bytes while the references exist can invalidate UTF-8, C-string, enum, and aliasing invariants.

The safe API therefore assumes the source memory remains stable for the entire decoded lifetime.

## 15.9 No promise against hostile concurrent mutation

A bounds-checked parser cannot defend a Rust reference against another process rewriting its referent after validation. This is a synchronization and ownership problem, not a serialization problem.

---

## 16. Shared-memory concurrency

## 16.1 Serialization safety is not publication safety

Completing `encode_into` only means the destination contains a valid canonical record at that instant. It does not make a racing reader safe.

## 16.2 Recommended immutable-slot protocol

The safest zero-copy model is immutable publication:

1. writer obtains exclusive ownership of an inactive slot;
2. writer encodes the full message into that slot;
3. writer publishes the slot index or generation with release ordering;
4. reader acquires the published index;
5. reader borrows the now-immutable slot;
6. slot is not reused until all readers have released it, using reference counting, epochs, RCU, or another ownership protocol.

This preserves the assumptions behind ordinary Rust references.

## 16.3 Double buffering

A practical design uses two or more slots:

```text
slot A: currently published and immutable
slot B: exclusively owned by writer
```

After encoding B, the writer atomically publishes B. A reclamation protocol determines when A may be reused.

## 16.4 Seqlocks require caution

A traditional seqlock may allow readers to race with non-atomic payload writes and retry afterward. Such a pattern is not automatically compatible with Rust and C++ data-race rules when the payload is accessed through ordinary references.

A safe seqlock integration generally needs either:

- atomic or volatile byte access through a specialized reader rather than ordinary references; or
- copying into a private snapshot before creating the validated view.

The latter sacrifices zero-copy decoding but preserves language-level safety.

## 16.5 Proposed runtime integration

A future shared-memory helper may expose:

```rust
let snapshot: StableSnapshot<'_> = slot.acquire()?;
let message = Message::parse(snapshot.as_bytes())?;
```

Constructing `StableSnapshot` would be the application-specific unsafe boundary guaranteeing that the bytes cannot change during the borrow.

---

## 17. Performance model

## 17.1 Root representation view

For a fixed-size `FromBytes` wire type, size/alignment checking and reference construction are O(1) with respect to buffer length.

## 17.2 Eager public projection

Direct public fields require eager work:

| Operation | Cost |
|---|---:|
| load fixed scalar | O(1) |
| map scalar enum | O(number of variants) in generated match, normally optimized |
| validate length bound | O(1) |
| validate UTF-8 `str` | O(string length) |
| find C-string NUL | O(position of first NUL), bounded by capacity |
| construct `U16Str` after length | O(1) |
| find `U16CStr` NUL | O(number of scanned code units), bounded by capacity |
| select tagged union | O(1) tag dispatch |
| decode nested schema | sum of nested field costs |
| validate zero tail | O(unused capacity) |
| validate zero padding | O(total padding checked) |

All fixed capacities place a compile-time upper bound on the work, but the practical cost still scales with scanned data.

## 17.3 No repeated string validation

The public projection caches the resulting borrowed string references. Accessing `message.name` does not revalidate UTF-8 or rescan for NUL.

## 17.4 Projection size

Native scalar fields are copied into the public view. A schema with many scalar fields therefore creates a larger stack value than a raw-backed accessor view would.

This is the deliberate price of direct field access and hidden endian wrappers. Large schemas may later opt into a separate backed-view mode with getters.

## 17.5 Encoding cost

Encoding is at least O(`WIRE_SIZE`) under the canonical default because the entire destination is zeroed. It additionally copies active string and fixed-buffer contents.

There is no intermediate allocation or second full-message copy.

## 17.6 Optimization expectations

Generated nested functions, scalar conversions, tag matches, and range calculations should be aggressively inlined. Layout offsets and capacities are compile-time constants.

Benchmarks should compare:

- hand-written C++ shared-memory access;
- hand-written Rust `zerocopy` access;
- generated `zero-schema` parsing;
- FlatBuffers and Cap’n Proto where relevant;
- encoding into cold and warm cache lines.

---

## 18. C++ interoperability

## 18.1 Wire types, not public Rust types, define interoperability

The public Rust type contains references and native projections and must never be mirrored in C++.

The generated hidden wire layout is the cross-language contract.

## 18.2 Native-endian C-layout profile

For same-host shared memory and existing C++ structs, the recommended profile is:

```rust
#[zero(endian = "native")]
```

with generated `repr(C)` field order and matching alignments.

This permits direct `U16CStr` views and straightforward C++ `std::uint16_t` arrays.

## 18.3 Explicit-endian scalar wrappers

The unpublished 0.1 C++ conformance harness mirrors explicit-endian scalar fields with byte arrays and native-value load/store helpers of identical size and alignment. This is test infrastructure, not a generated-header feature or public C++ library. The Rust public model still exposes native scalars.

## 18.4 Conceptual C++ mapping

Rust schema:

```rust
#[derive(ZeroSchema)]
#[zero(endian = "native")]
pub struct FileConfig<'a> {
    pub flags: u32,

    #[zero(capacity = 260)]
    pub path: &'a U16CStr,
}
```

Conceptual C++ wire declaration:

```cpp
struct FileConfigWire {
    std::uint32_t flags;
    std::uint16_t path[260];
};
```

Use `std::uint16_t` for the exact 16-bit storage contract. `wchar_t` is platform-dependent and is not a portable substitute. `char16_t` may be used only with matching layout assertions and agreed aliasing/API rules.

## 18.5 Tagged union mapping

Conceptual generated C++:

```cpp
enum class ConfigKind : std::uint16_t {
    Memory = 1,
    File = 2,
};

union ConfigPayloadWire {
    MemoryConfigWire memory;
    FileConfigWire file;
};

struct ConfigWire {
    ConfigKindRaw tag; // raw integer or generated checked wrapper
    ConfigPayloadWire payload;
};
```

Even when C++ exposes an `enum class`, the wire validation boundary should treat the incoming storage as an integer because arbitrary shared-memory bytes may not correspond to a declared enumerator.

## 18.6 Layout assertions

Every C++ integration should compile assertions such as:

```cpp
static_assert(sizeof(MessageWire) == ZERO_SCHEMA_MESSAGE_SIZE);
static_assert(alignof(MessageWire) == ZERO_SCHEMA_MESSAGE_ALIGN);
static_assert(offsetof(MessageWire, sequence) == ZERO_SCHEMA_MESSAGE_SEQUENCE_OFFSET);
static_assert(offsetof(MessageWire, config) == ZERO_SCHEMA_MESSAGE_CONFIG_OFFSET);
```

Rust tests should assert the same constants through `size_of`, `align_of`, and `offset_of`.

## 18.7 Unpublished conformance harness

Version 0.1 does not generate C++ headers and exposes no stable C or C++ API. The repository's unpublished harness generates temporary C++ declarations from its schema corpus, compiles them for the same target ABI, and calls a narrow `extern "C"` test ABI. That ABI uses raw pointers plus explicit lengths, returns integer status codes rather than throwing across the boundary, and is confined to the tested native-endian and explicit-endian scalar profiles.

The harness compares Rust descriptors with C++ `sizeof`, `alignof`, and `offsetof`, exchanges canonical golden bytes in both directions, and checks closed scalar/tagged values, strings, padding, and inactive payload storage covered by the corpus. Its symbols, status values, generated declarations, and fixture inventory are deliberately unpublished and may change without semver impact.
## 18.8 Existing ABI versus generated ABI

`repr(C)` follows the target platform’s C layout rules, not a universal cross-target ABI. Existing C++ compiler flags, packing pragmas, unusual enum options, and target-specific primitive alignment can still cause mismatch.

The supported contract is:

> Matching target, matching declared layout policy, and passing generated static layout assertions.

---

## 19. Errors and diagnostics

`zero-schema` separates schema-definition failures from data-processing failures:

- an invalid declaration or invalid `#[zero(...)]` option is a compile-time error;
- malformed input bytes produce a generated decode error;
- an invalid logical value or unsuitable destination produces a generated encode error.

The runtime path must not use panics for malformed external data or ordinary validation failures. It returns the first structured error and never returns a partially validated public value. The core error representation must work without heap allocation.

## 19.1 Generated error surface

Every schema exposes concrete operation-specific errors through its generated methods and associated trait types:

```rust
impl<'a> Message<'a> {
    pub fn parse(
        bytes: &'a [u8],
    ) -> Result<Self, MessageDecodeError>;

    pub fn parse_prefix(
        bytes: &'a [u8],
    ) -> Result<(Self, &'a [u8]), MessageDecodeError>;
}

impl Message<'_> {
    pub fn encode(&self) -> Result<zero_schema::AlignedBytes<EncodedAlignment, { ENCODED_SIZE }>, MessageEncodeError>;

    pub fn encode_into(
        &self,
        destination: &mut [u8],
    ) -> Result<(), MessageEncodeError>;
}
```

Decode and encode errors are separate because their failure sets are different. For example, arbitrary bytes may contain invalid UTF-8 or an unknown enum value, while an encoding input already contains a valid Rust `&str` and a declared Rust enum variant. Encoding can instead fail because a valid logical string exceeds its fixed wire capacity or because a redundant external tag disagrees with its payload.

Generated public error enums should be marked `#[non_exhaustive]`. Adding a validation category or a schema field must not require downstream consumers to exhaustively match every generated variant. Applications that require a closed application-level error API can convert the generated error at their boundary.

## 19.2 Decode failure contract

Decoding is eager and fail-fast:

1. root size and alignment are checked before a wire reference is formed;
2. direct fields are projected in declaration order;
3. built-in validity is checked before a field validator runs;
4. a tagged-union tag is validated before any payload is selected;
5. only the selected payload is decoded;
6. nested schemas are decoded depth-first when their containing field is reached;
7. the whole-value validator runs only after every field has been projected and the public value has been constructed.

The first failing step returns an error. Errors are not accumulated. This keeps work bounded, preserves a no-allocation implementation, and prevents custom validators from observing partially valid public values.

For a fixed generated implementation and immutable input, error selection is deterministic. Root layout errors take precedence over semantic errors; field errors follow declaration order; a union-tag error takes precedence over a selected-payload error; and a whole-value custom error is considered last. Reordering independent validation across generated versions is not a wire-compatibility guarantee.

A representative generated error is:

```rust
#[non_exhaustive]
pub enum MessageDecodeError {
    Layout(zero_schema::LayoutError),
    InvalidBool {
        field: &'static str,
        value: u8,
    },
    LengthOutOfBounds {
        field: &'static str,
        length: usize,
        capacity: usize,
    },
    InvalidUtf8 {
        field: &'static str,
        source: core::str::Utf8Error,
    },
    MissingNul {
        field: &'static str,
    },
    NonZeroTail {
        field: &'static str,
        offset: usize,
    },
    NonZeroPadding {
        offset: usize,
    },
    State(StateDecodeError),
    Header(HeaderDecodeError),
    Config(ConfigDecodeError),
    Custom {
        field: Option<&'static str>,
        source: zero_schema::ValidationFailure,
    },
}
```

This example is conceptual. The derive emits only variants reachable for that schema. A scalar-enum error records the unknown raw value; a tagged-enum error records an unknown tag or wraps the selected variant's payload error. Concrete nested variants replace an undefined type-erased `NestedDecodeError`.

Decode failures include:

| Category | Condition |
|---|---|
| layout | input length is not the required wire size, or the starting address is insufficiently aligned |
| boolean | raw storage is neither `0` nor `1` |
| scalar enum | raw integer does not name a declared variant |
| length | a length prefix exceeds capacity or cannot be converted safely |
| UTF-8 | the selected `str` bytes are not valid UTF-8 |
| terminator | no narrow or wide NUL exists within fixed capacity |
| canonical tail | `tail = "zero"` is requested and an unused byte or code unit is nonzero |
| canonical padding | `padding = "zero"` is requested and generated padding is nonzero |
| tagged union | the tag is unknown in the closed-union model |
| nested schema | a child field or selected payload fails its own decode |
| custom validation | a field or whole-value validator returns `ValidationFailure` |

No public value is returned on failure. On success, all direct field access is infallible because the returned value contains only already-validated Rust values and references.

Decode errors do not detect hostile concurrent mutation. The stable-snapshot requirement in Sections 15.8, 15.9, and 16 remains a precondition; another process rewriting validated bytes afterward is a synchronization violation, not a recoverable schema error.

## 19.3 Encode failure contract

Encoding is fail-fast. `encode()` owns and withholds its storage until success;
`encode_into` mutates caller-provided storage. A representative generated error is:

```rust
#[non_exhaustive]
pub enum MessageEncodeError {
    Layout(zero_schema::LayoutError),
    CapacityExceeded {
        field: &'static str,
        length: usize,
        capacity: usize,
    },
    TagMismatch {
        field: &'static str,
        tag_field: &'static str,
        declared: ConfigKind,
        selected: ConfigKind,
    },
    Header(HeaderEncodeError),
    Config(ConfigEncodeError),
    Custom {
        field: Option<&'static str>,
        source: zero_schema::ValidationFailure,
    },
}
```

Encode failures include:

| Category | Condition |
|---|---|
| destination layout | destination size or address alignment is wrong |
| capacity | string bytes or code units, including a required terminator, exceed fixed storage |
| length representation | a logical length cannot be represented by the configured length type |
| external tag | a sibling tag does not equal the selected public union variant's tag |
| nested schema | a child value cannot be encoded into its fixed representation |
| custom validation | a field, cross-field, or whole-value validator rejects the logical value |

The public Rust type already excludes invalid UTF-8, invalid public enum discriminants, and unterminated `CStr`/`U16CStr` values. Those are decode-only failures. Capacity and external-tag consistency remain runtime checks because the ordinary Rust field types do not encode fixed capacity or redundant cross-field equality in their types.

The destination-state contract is defined in Section 11.2: size or alignment failure leaves the destination untouched, while any later `Err` may leave it zeroed or partially encoded. `Err` never authorizes publication of the destination.

## 19.4 Nested errors and logical paths

Parents wrap concrete child errors rather than erasing them:

```rust
pub enum MessageDecodeError {
    // Built-in field failures omitted.
    Header(HeaderDecodeError),
    Config(ConfigDecodeError),
}

pub enum ConfigDecodeError {
    UnknownTag(u16),
    Memory(MemoryConfigDecodeError),
    File(FileConfigDecodeError),
}
```

A nested chain carries one logical path segment at each level. Formatting the chain above may produce:

```text
Message.config.File.path: missing U16 NUL terminator
```

Paths use public schema, field, and variant names, never hidden wire-module names or byte-offset implementation details. A built-in leaf records its local field statically. A nested wrapper contributes the containing field or selected variant and exposes the child as its source.

Path formatting must not allocate. `Display` writes segments directly to the supplied formatter by walking the concrete source chain. Under `std`, generated errors implement `std::error::Error` and return the concrete nested error from `source()`. The `no_std` core retains the same structured chain without requiring a rendered `String`.

## 19.5 Custom validation failures

Custom validators return the common `zero_schema::ValidationFailure`. It contains at least:

- an application-defined numeric code;
- a static or otherwise allocation-free diagnostic message in the core configuration;
- optional static validation metadata supplied by `ValidationContext`.

A generated schema wrapper adds the field or whole-value context:

```rust
MessageDecodeError::Custom {
    field: Some("name"),
    source: ValidationFailure::new(1001, "name is not permitted"),
}
```

Field validators run only after built-in conversion. A `&str` validator therefore always receives valid UTF-8, and a tagged-union validator receives only a known selected variant. Whole-value validators run after all fields succeed. The same logical validation failure may be wrapped by decode or encode errors, depending on which operation invoked the validator.

## 19.6 Why errors are generated per schema

Per-schema errors are a static-composition choice, not a memory-safety requirement. They serve four goals:

1. **Concrete nested sources.** `MessageDecodeError::Header` can contain `HeaderDecodeError` directly, and `ConfigDecodeError::File` can contain `FileConfigDecodeError` directly.
2. **No allocation or boxing.** Recursive error context does not require `Box<dyn Error>`, an allocated path, or an owned message.
3. **Reachable failure sets.** A schema without strings need not expose `InvalidUtf8` or `MissingNul`; an internally tagged schema cannot produce an external `TagMismatch`.
4. **Typed application handling.** Applications can distinguish layout corruption, malformed external data, local capacity failures, and policy validation without parsing diagnostic text.

The associated error types on `DecodeWire` and `EncodeWire` make this composition explicit. A parent is generic over a child's concrete error until its own derive creates the wrapping variant.

The trade-offs are:

- more generated public types and variants;
- larger compile-time and code-size footprints for schemas with many distinct nested types;
- generated error variants can change when a schema changes;
- heterogeneous generic callers must retain an associated error type or convert errors at an application boundary.

A single global flat error containing `{ schema, field, kind }` could also be allocation-free, but it would either erase concrete child error types or recreate a separate tagged representation for every possible nested source. The initial design keeps concrete generated errors and adds a common inspection interface for generic consumers.

## 19.7 Common error inspection

The runtime crate provides stable operation-independent metadata:

```rust
#[non_exhaustive]
pub enum ErrorKind {
    Layout,
    InvalidBool,
    UnknownEnumValue,
    LengthOutOfBounds,
    InvalidUtf8,
    MissingNul,
    NonZeroTail,
    NonZeroPadding,
    UnknownUnionTag,
    CapacityExceeded,
    TagMismatch,
    CustomValidation,
}

pub enum ErrorPathSegment {
    Field(&'static str),
    Variant(&'static str),
}

pub trait SchemaError: core::error::Error + 'static {
    fn kind(&self) -> ErrorKind;
    fn schema(&self) -> &'static str;
    fn segment(&self) -> Option<ErrorPathSegment>;
    fn child(&self) -> Option<&dyn SchemaError>;
    fn validation_code(&self) -> Option<u32> { None }
}
```

These names and signatures are the 0.1 inspection contract. Generic logging and metrics match `ErrorKind`, inspect `validation_code`, and walk `segment`/`child`; they do not parse `Display`.

`Display` is human-readable and stable enough for diagnostics, but is not a machine protocol. `ErrorKind`, custom validation codes, and generated layout/schema identifiers are the machine-readable surface. Public generated error enums are non-exhaustive so the runtime may add detail without invalidating correct catch-all handling.

## 19.8 Compile-time diagnostic stages

Invalid schemas are rejected before usable encode or decode implementations are emitted. Validation proceeds in stages:

1. **Attribute parsing.** Reject unknown options, malformed values, duplicate singleton options, and options placed on the wrong item category.
2. **Local shape validation.** Classify the item and each syntactically recognized field, then enforce the applicability table in Section 7.4.
3. **Intra-item resolution.** Resolve borrow lifetimes, sibling `tag_field` identifiers, variant tags, duplicate discriminants, and other references visible within the declaration.
4. **Generated type assertions.** Ask rustc to prove traits and associated-type equality for arbitrary nested schema, scalar-enum, tagged-union, validator, and external-tag types.
5. **Layout assertions.** Let rustc evaluate size, alignment, union-capacity, and representation constraints expressed through generated types and constants.

Stages 1 through 3 produce direct proc-macro diagnostics. Stages 4 and 5 use generated assertions whose tokens carry the original field, type, or attribute span. A procedural macro cannot query rustc's trait solver during expansion, so cross-item type relationships must not be guessed.

The derive should combine independent direct errors when doing so does not require an invalid intermediate representation. It must not continue to emit normal wire and runtime implementations after a fatal local schema error merely to produce secondary generated-code noise.

## 19.9 Required compile-time rejections

The derive must reject or cause a span-preserving rustc error for:

- an unknown, duplicate, contradictory, misplaced, or inapplicable `#[zero]` option;
- missing `capacity` on a supported borrowed string;
- `capacity` on a primitive, enum, fixed byte array, ordinary nested schema, or any other non-string field;
- zero capacity on `CStr` or `U16CStr`;
- capacity too large for `len_type`;
- `len_type` on anything other than `str` or `U16Str`;
- an unsupported public field type;
- a nested field that does not implement the required schema traits;
- a validator whose path or signature does not type-check for the projected value;
- a `range` or `must_equal` expression that does not type-check for the field;
- a missing explicit scalar-enum discriminant;
- a duplicate or out-of-range scalar-enum discriminant;
- a missing, duplicate, or type-incompatible tagged-union variant tag;
- a union variant with an unsupported shape;
- `tag_field` on a field that is not a tagged union;
- a missing external tag sibling;
- an external tag sibling that is not a scalar schema enum;
- an external tag type that differs from the union's associated tag type;
- non-power-of-two or target-invalid alignment;
- non-native direct `U16Str` or `U16CStr` storage;
- multiple lifetimes without an unambiguous `borrow` selection;
- `usize`, pointers, or other target-dependent storage without an explicit future escape hatch;
- inline recursion that would produce an infinitely sized wire type.

Known unsupported standard-library containers such as `String`, `Vec<T>`, `Box<T>`, and `Option<T>` should receive targeted macro diagnostics. An arbitrary path that is syntactically eligible as a nested schema is checked through generated trait bounds, because it may be defined in another crate.

## 19.10 Diagnostic examples

Inapplicable field option:

```rust,compile_fail
#[derive(ZeroSchema)]
struct BadCapacity {
    #[zero(capacity = 16)]
    count: u32,
}
```

The primary diagnostic points at `capacity` and states that it is valid only on supported borrowed string fields. The field type may be shown as secondary context.

Invalid `tag_field` carrier:

```rust,compile_fail
#[derive(ZeroSchema)]
struct BadCarrier<'a> {
    kind: ConfigKind,

    #[zero(tag_field = kind)]
    name: &'a str,
}
```

The diagnostic points at `tag_field` and states that `name` must implement the tagged-union schema trait.

Missing sibling:

```rust,compile_fail
#[derive(ZeroSchema)]
struct MissingTag<'a> {
    #[zero(tag_field = kind)]
    config: Config<'a>,
}
```

The diagnostic points at `kind` and reports that no sibling field with that name exists.

Wrong associated tag type:

```rust,compile_fail
#[derive(ZeroSchema)]
struct WrongTag<'a> {
    kind: OtherKind,

    #[zero(tag_field = kind)]
    config: Config<'a>, // Config::Tag is ConfigKind
}
```

The generated assertion requires `Config<'a>: TaggedUnion<'a, Tag = OtherKind>` and carries the `tag_field` or `config` span, allowing rustc to report the actual `ConfigKind` versus `OtherKind` mismatch without pointing primarily into the hidden support module.

---

## 20. Implementation architecture

## 20.1 Crate organization

```text
zero-schema/
    src/
        lib.rs
        schema.rs
        decode.rs
        encode.rs
        error.rs
        layout.rs
        validation.rs
        buffer.rs
        strings.rs
        tagged.rs
        __private.rs

zero-schema-derive/
    src/
        lib.rs
        attrs.rs
        parse.rs
        ir.rs
        classify.rs
        validate.rs
        generate/
            mod.rs
            wire.rs
            decode.rs
            encode.rs
            errors.rs
            layout.rs
            tagged.rs

zero-schema-cpp/          # later tooling
    src/
        main.rs
        emit_cpp.rs
        descriptor.rs
```

## 20.2 Proc-macro input model

The derive parses a `syn::DeriveInput` and builds an internal representation containing:

- item kind: struct, scalar enum, tagged enum;
- generics and selected borrow lifetime;
- container options;
- ordered fields or variants;
- logical field classification;
- capacity, endianness, alignment, and validation policies;
- external dependencies such as tag-field references;
- generated names and source spans.

## 20.3 Field classification and semantic validation

The macro recognizes built-in logical types syntactically:

- fixed-width primitives;
- `bool`;
- `&str`;
- `&CStr`;
- `&U16Str`;
- `&U16CStr`;
- `&[u8; N]`.

The internal field model records the recognized category, the original `syn::Type`, every option with its span, and any unresolved cross-item requirements. Known unsupported standard-library shapes such as `String`, `Vec<T>`, `Box<T>`, `Option<T>`, raw pointers, and dynamic slices are rejected directly. Other paths are provisionally classified as nested schemas because they may be user-defined types from another crate.

Semantic validation has two boundaries:

1. **Macro-known validation.** The derive checks option placement and applicability, literal ranges, duplicate options, local discriminants and tags, sibling names, lifetimes, and every other relationship visible in the `DeriveInput`.
2. **Rust-known validation.** Generated assertions require arbitrary paths to implement `ZeroSchemaType`, `DecodeWire`, `EncodeWire`, `ScalarEnum`, or `TaggedUnion` as appropriate, and require associated tag types to be equal.

Every parsed option must be consumed by exactly one applicable rule. An unconsumed option is an internal macro bug; it must not be dropped from code generation as though it were valid.

For an external tag, the derive conceptually emits a zero-runtime-cost assertion:

```rust
#[allow(dead_code)]
fn __assert_message_config_external_tag<'a>()
where
    Config<'a>: zero_schema::__private::TaggedUnion<
        'a,
        Tag = ConfigKind,
    >,
    ConfigKind: zero_schema::__private::ScalarEnum,
{
}
```

The assertion tokens are emitted with the source span of `tag_field`, the payload type, or the target field type by using span-aware quotation. The same technique validates nested schema traits and custom validator signatures. This lets rustc report cross-crate type facts while keeping the primary diagnostic attached to user-authored code.

Code generation begins only after macro-known validation has produced a coherent internal representation. Independent `syn::Error` values should be combined so a user can fix multiple local declaration errors in one compilation, but generated wire implementations must not be emitted from an invalid field model.

## 20.4 Hidden wire modules

For a public type `Message`, generated support may live in:

```rust
#[doc(hidden)]
pub mod __zero_schema_message {
    pub struct Wire<'a> { /* ... */ }
    pub enum DecodeError { /* ... */ }
    pub enum EncodeError { /* ... */ }
    pub static LAYOUT: LayoutDescriptor = /* ... */;
}
```

The module and wire type may need to be public-but-doc-hidden because associated wire types must be nameable across crate boundaries for nested schemas.

## 20.5 Primitive wrapper generation

The runtime crate should provide a family of hidden primitive wire wrappers rather than generating duplicate definitions per field:

```text
NativeU16, LittleU16, BigU16
NativeU32, LittleU32, BigU32
NativeU64, LittleU64, BigU64
...
```

Each wrapper:

- has the intended scalar size;
- preserves the selected C-layout alignment;
- is all-bit-valid;
- derives the relevant `zerocopy` traits;
- provides native-value `get` and `set`/encode helpers.

## 20.6 Generated wire derives

Every generated aggregate wire type derives, through the consuming crate's resolved direct `zerocopy` dependency:

```rust
zerocopy::FromBytes
zerocopy::KnownLayout
zerocopy::Immutable
```

Aggregate `IntoBytes` is neither required nor promised. The implementation does not manually implement these unsafe conversion traits.

## 20.7 Generated decode implementation

Generated decoding constructs `DecodeInput::from_exact` or `DecodeInput::from_prefix`, then calls the doc-hidden `DecodeWire::decode_at`. Struct fields are projected in declaration order; nested ranges preserve their exact source bytes for padding checks; selected tagged payloads are decoded only after tag validation. This is the exact composition model in Section 14.2, not the earlier conceptual `decode_wire(&Wire)` signature.
## 20.8 Generated encode implementation

Generated `encode_into` rejects incorrect size before misalignment and leaves the destination untouched for either layout error. It then runs `EncodeWire::validate_encode` before creating one root `Prezeroed`, which fills the destination once, and calls `EncodeWire::encode_at`; nested encoders receive `Prezeroed::subrange` capabilities and do not allocate, refill, or copy an intermediate message. A later error is nontransactional and may leave zeroed or partially written bytes as specified in Section 11.2.
## 20.9 Scalar-enum and tagged-union traits

The implemented cross-item contracts are:

```rust
pub trait ScalarEnum: ZeroSchemaType<Wire: ScalarWire> {
    fn from_raw(raw: <Self::Wire as ScalarWire>::Repr) -> Option<Self>;
    fn to_raw(&self) -> <Self::Wire as ScalarWire>::Repr;
}

pub trait TaggedUnion: EncodeWire {
    type Tag: ScalarEnum;
    type PayloadWire: zerocopy::FromBytes
        + zerocopy::KnownLayout
        + zerocopy::Immutable
        + 'static;

    fn tag(&self) -> Self::Tag;
    fn validate_payload_encode(&self) -> Result<(), Self::EncodeError>;
    fn encode_payload_at(
        &self,
        destination: &mut Prezeroed<'_>,
    ) -> Result<(), Self::EncodeError>;
}

pub trait DecodeTaggedUnion<'src>: TaggedUnion + DecodeWire<'src> {
    fn decode_payload(
        tag: &Self::Tag,
        input: DecodeInput<'src, Self::PayloadWire>,
    ) -> Result<Self, Self::DecodeError>;
}
```

`ScalarWire` and the encoding traits are sealed or doc-hidden implementation surfaces. External tags require associated `Tag` equality, so a scalar enum from the wrong domain is rejected even when its integer representation matches.

## 20.10 Layout metadata

Every derive emits the `LayoutDescriptor` referenced by `ZeroSchemaType::LAYOUT`, including the implemented type, field, string, enum, variant, byte-range, size, alignment, stride, endian, padding, and tail descriptors. Parsing and encoding use generated constants rather than walking the descriptor. The descriptor is used by diagnostics and the unpublished conformance/golden tests; 0.1 does not define a stable descriptor serialization, schema fingerprint, or header generator.

## 20.11 Macro hygiene

The derive:

- resolves renamed `zero-schema` and direct `zerocopy` crates with `proc_macro_crate`, with `#[zero(crate = path)]` overriding only the runtime path;
- emits absolute paths and module-scope sibling support items with visibility derived from the schema's visibility;
- preserves supported generics and where clauses;
- derives deterministic hidden and public support names from the logical identifier, stripping raw `r#` spelling;
- attaches diagnostics to original spans and rejects collisions with generated API names.

## 20.12 `no_std`

The 0.1 runtime is `#![no_std]`; `alloc` and `std` are additive features. Core parsing, encoding, descriptors, validation, and generated error paths allocate nothing. `std` enables standard error integration and allocation conveniences, not a different wire contract.

---

## 21. Testing and verification

## 21.1 Compile-fail UI tests

Use `trybuild` or rustc UI tests to snapshot both the rejection and the primary diagnostic span. The matrix must cover:

- unknown options, malformed values, duplicate singleton options, contradictory options, and options on the wrong item category;
- every field option on at least one valid and one invalid logical field category;
- missing string capacity, zero C-string capacity, and every `capacity`/`len_type` boundary;
- `capacity` on primitives, scalar enums, tagged enums, fixed byte arrays, and nested schemas;
- `tag_field` on a non-union field;
- a missing external-tag sibling;
- primitive, string, ordinary-schema, and scalar-enum-of-the-wrong-domain tag targets;
- an external tag declared before and after its payload, both of which must compile;
- unsupported standard-library field types and arbitrary nested types missing schema traits;
- missing, duplicate, out-of-range, and type-incompatible enum or union tags;
- unsupported union variant shapes;
- invalid validator signatures and invalid `range` or `must_equal` expressions;
- ambiguous lifetimes, invalid alignment, target-dependent types, inline recursion, and wide-string endianness.

Tests for generated trait assertions must verify that the primary span remains on the user field, type, or attribute rather than in the hidden support module. Each rejection family must have a nearby compile-pass counterpart to prevent an over-broad diagnostic rule from excluding valid schemas.

## 21.2 Round-trip property tests

For generated values within capacity:

```text
decode(encode(value)) == value
```

where equality is logical rather than pointer identity.

## 21.3 Arbitrary-byte fuzzing

The committed root inventory is `test-fixtures/schema-corpus/inventory.csv`. It is UTF-8 with LF line endings, one header, no blank or comment rows, no quoting, and exactly these eleven comma-free columns:

```text
root_id,type_key,fuzz_target,selector,golden_path,golden_len,golden_sha256,valid_seed_path,valid_seed_sha256,invalid_seed_path,invalid_seed_sha256
```

Root IDs and selectors are decimal; IDs are unique and selectors are dense from 1 through the target count (and never exceed 256). Paths are normalized workspace-relative paths without `..`; hashes are 64 lowercase hexadecimal SHA-256 digits. The 0.1 inventory has exactly six rows: roots 1–3 select `CorpusCode16Be`, `CorpusMessage`, and `CorpusCode8` through `parse_message` selectors 1–3; root 4 selects `ExternalCorpusMessage` through `parse_external_tag/1`; root 5 selects `FuzzAllStrings` through `parse_all_strings/1`; and root 6 selects the `CorpusMessage` round-trip campaign through `roundtrip_message/1`. The checked inventory and the schema corpus root registry must have identical root-ID coverage.

Every row names `test-fixtures/schema-corpus/golden/<root_id>.bin` with its exact length and hash, plus committed `fuzz/corpus/<target>/<selector>-valid` and `-invalid` seeds with independently verified hashes. Inventory tests read and hash every file, require the valid seed to parse successfully and the invalid seed to fail semantically, and reject a missing, duplicate, sparse, extra, or misdirected selector. Hashing and filesystem access are test-only; normal fuzz dispatch has no hashing path.

The four campaign targets are `parse_message`, `parse_external_tag`, `parse_all_strings`, and `roundtrip_message`. Dispatch is bounded and deterministic: `selector = usize::from(input.first().copied().unwrap_or(0)) % count`, `payload = input.get(1..).unwrap_or(&[])`; the selected `make_buffer_for!(FullyConcreteType)` storage is zero-filled, at most `WIRE_SIZE` payload bytes are copied, missing bytes remain zero, and excess bytes are ignored. Exactly one branch parses the normalized bytes twice and compares stable success or error observations; the round-trip branch additionally encodes and reparses each success. Dedicated tests exercise raw empty, selector-only, short, exact, and oversized public inputs and malformed external-tag and string cases.

`ci/run-fuzz.sh` accepts exactly one of the four target names above and runs it for exactly 10,000 executions. It verifies locked metadata with `nightly-2025-11-15`, records and rechecks the standalone fuzz lock hash, copies the reviewed corpus to a temporary mutable input, and places discoveries only under the absolute target artifact directory. The committed corpus is never mutated by a campaign. After every run, `ci/check-fuzz-clean.sh` audits tracked, untracked, and ignored entries in the reviewed corpus and both artifact trees; any discovery or lock/corpus change fails the gate and must be reviewed into the inventory before rerunning all gates.

## 21.4 Miri

The 0.1 gate runs only `tests/miri.rs` with `nightly-2025-11-15`. It covers exact and
prefix parsing, every borrowed string projection (including embedded NUL, non-UTF-8
C bytes, and unpaired wide surrogates), selected internal and external tagged payload
ranges, nested structured errors, `AlignedBytes` methods, `make_buffer_for!`,
encode/decode round trips, original-byte padding scans, and aligned storage.
Dedicated malformed cases exercise misaligned decode and encode roots, invalid UTF-8,
excessive wide length, missing wide NUL, nonzero wide tails, unknown internal and
external tags, invalid selected payload booleans, and error source/path identity. No
Miri test casts an aggregate or union reference to bytes; proptest, trybuild, the
counting allocator, and C++ FFI are separate gates.

## 21.5 Cross-language layout tests

The unpublished C++ harness must compile and run its checked profiles and compare descriptor inventory, `sizeof`, `alignof`, every recorded `offsetof`, scalar bytes, union layout, string storage/termination, Rust-to-C++ and C++-to-Rust canonical goldens, and stable nonzero status results for rejected input. Only combinations present in the schema corpus are claimed supported.

## 21.6 Golden byte tests

The checked-in deterministic corpus is the byte-level oracle for each tested schema/profile. Rust and C++ must both match the same canonical bytes, including padding, tails, and inactive tagged storage; descriptor inventory and golden regeneration must be clean before release.

## 21.7 Cross-endian testing

Use cross-compilation and emulation where practical to verify explicit-endian numeric fields. Direct `U16Str` and `U16CStr` tests must confirm compile-time rejection or appropriate behavior when wire and native endian differ.

## 21.8 Unsafe-code audit

Keep a source-level inventory of every unsafe block. Each unsafe block should have a local `SAFETY:` comment restating the invariant established by generated layout and bounds checks.

## 21.9 Benchmarks and verification gates

Criterion benchmarks cover Rust parse and encode profiles, and the unpublished C++ harness supplies comparable native operations; results are measurements, not latency guarantees. Push and pull-request verification requires the focused workspace tests, compile-pass/fail matrix, property tests, deterministic goldens and exact six-row root/seed inventory, C++ profile build/run, the dedicated malformed-input Miri target, no-allocation instrumentation, `no_std`/wide-target checks, benchmarks compiling, and package-policy checks. Pull-request verification additionally runs all four 10,000-run fuzz campaigns. Ordinary CI builds and verifies package archives, including isolated package-pair consumers, but does not publish them or upload a package handoff. A fuzz discovery, dirty corpus/artifact tree, changed standalone lock, missing seed, hash mismatch, or root/selector coverage drift fails verification. A profile not exercised by those gates is not claimed by 0.1.

---

## 22. Schema evolution

Fixed-layout shared-memory formats require explicit evolution rules.

## 22.1 Version fields

A root schema should normally include a version:

```rust
pub version: u16,
```

A custom validator can enforce version-dependent rules.

## 22.2 Size changes

Adding an inline field changes offsets, size, and possibly alignment. It is not automatically backward compatible.

Common strategies:

- define a new root schema type;
- reserve explicit zeroed byte ranges;
- version the slot header and select a schema by version;
- parse a stable prefix and then a version-specific suffix.

## 22.3 New enum values

Closed scalar enums reject unknown values. Adding a value is compatible only after every reader understands it; 0.1 provides no open-enum mode.

## 22.4 New union variants

Closed tagged unions reject unknown tags. Adding a variant is compatible only after every reader understands it; 0.1 provides no open-union or unknown-payload preservation mode.

## 22.5 Schema fingerprints

Schema fingerprints are not part of 0.1. `LayoutDescriptor` is runtime metadata, not a canonical serialized form, and its address or debug representation must not be used as a compatibility identifier. Applications that need negotiation must version their control block explicitly.

---

## 23. Initial feature set and roadmap

## 23.1 Version 0.1 core

- per-type `ZeroSchema` derive;
- named-field structs;
- fixed-width integer and float primitives;
- `bool` as checked `u8`;
- `&str`, `&CStr`, `&U16Str`, `&U16CStr`;
- borrowed fixed `[u8; N]`;
- fieldless `u8`/`u16`/`u32` enums;
- internally tagged unions;
- sibling externally tagged unions;
- one-payload-field or unit union variants;
- inline nested schemas;
- native/little/big numeric endian;
- type and field alignment;
- canonical zeroed encoding;
- eager validation;
- generated errors and layout constants;
- `no_std` core;
- fuzzing, Miri, and C++ layout tests.

## 23.2 Version 0.2 candidates — explicitly out of scope

Everything in this subsection is outside the implemented and supported 0.1 contract: generated C++ headers; open scalar enums; open tagged unions and unknown-payload preservation; fixed arrays of nested schemas; lazy raw-backed views; transactional writers; valid-UTF-16 logical fields; reserved/checksum attributes; schema fingerprints; and richer validator contexts. These are not promises or release commitments.

## 23.3 Later research

- relative offsets and arena-backed variable-size data;
- recursive graphs;
- packed-layout backend;
- atomic-field and stable-snapshot integrations;
- zero-copy read-only variable-length sequences;
- code generation for additional languages;
- in-place migration between schema versions.

---

## 24. Known limitations and deliberate trade-offs

### 24.1 Direct fields require a projection object

The parsed `Message<'a>` is not just one pointer to the raw wire. It contains copied native scalars, borrowed references, and nested enum values. This is necessary to provide direct field access without exposing wire wrappers or fallible getters.

### 24.2 Encoding is not literally zero-copy

Any inline destination must receive bytes. The project avoids intermediate buffers and duplicate full-message copies, but strings and fields are still written.

### 24.3 `U16CStr` is native-endian only

The zero-copy public type is tied to native `u16` representation. Portable non-native-endian UTF-16 needs a different public view or a copy.

### 24.4 C-compatible does not mean universally identical

Target ABI, compiler flags, and packing pragmas matter. Layout assertions remain mandatory.

### 24.5 Direct public scalar fields cost projection space

A very large record with hundreds of scalar fields produces a similarly large public value. A getter-backed mode may be preferable for those schemas.

### 24.6 Eager string validation is not O(1)

UTF-8 and NUL scans are linear in the bounded field content or capacity.

### 24.7 Concurrent mutation remains outside the type system

The framework cannot prove that another process will not modify a memory mapping. Applications need an immutable publication protocol.

### 24.8 External tags create redundant public state

The encoder must check consistency. Internal tags are cleaner for newly designed formats.

---

## 25. Full example

```rust
use std::ffi::CStr;
use widestring::{u16cstr, U16CStr};
use zero_schema::{ValidationContext, ValidationFailure, ValidationResult, ZeroSchema};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[repr(u8)]
#[zero(endian = "native")]
pub enum State {
    Initializing = 0,
    Ready = 1,
    Failed = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "native")]
pub enum ConfigKind {
    Memory = 1,
    File = 2,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct Header<'a> {
    pub version: u16,

    #[zero(capacity = 32, tail = "zero")]
    pub producer: &'a CStr,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct MemoryConfig {
    pub capacity_bytes: u64,
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native", validate_with = validate_file)]
pub struct FileConfig<'a> {
    pub flags: u32,

    #[zero(capacity = 260, tail = "zero")]
    pub path: &'a U16CStr,
}

fn validate_file(
    value: &FileConfig<'_>,
    _: &ValidationContext<'_>,
) -> ValidationResult {
    if value.path.is_empty() {
        return Err(ValidationFailure::new(2001, "file path must not be empty"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(tag = ConfigKind, tail = "zero")]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),

    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
}

#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(
    endian = "native",
    align = 64,
    padding = "zero",
    validate_with = validate_message
)]
pub struct Message<'a> {
    pub sequence: u64,
    pub state: State,
    pub header: Header<'a>,

    #[zero(capacity = 64, len_type = u16, tail = "zero")]
    pub name: &'a str,

    #[zero(align = 16)]
    pub config: Config<'a>,
}

fn validate_message(
    value: &Message<'_>,
    _: &ValidationContext<'_>,
) -> ValidationResult {
    if value.state == State::Ready && value.header.version == 0 {
        return Err(ValidationFailure::new(
            1001,
            "ready messages require a nonzero version",
        ));
    }
    Ok(())
}

fn encode_example() -> Result<zero_schema::AlignedBytes<EncodedAlignment, { ENCODED_SIZE }>, MessageEncodeError> {
    let value = Message {
        sequence: 42,
        state: State::Ready,
        header: Header {
            version: 3,
            producer: c"worker-service",
        },
        name: "active configuration",
        config: Config::File(FileConfig {
            flags: 0x03,
            path: u16cstr!(r"C:\data\cache.bin"),
        }),
    };

    value.encode()
}

fn decode_example(bytes: &[u8]) -> Result<(), MessageDecodeError> {
    let message = Message::parse(bytes)?;

    println!("sequence: {}", message.sequence);
    println!("state: {:?}", message.state);
    println!("producer: {:?}", message.header.producer);
    println!("name: {}", message.name);

    match message.config {
        Config::Memory(memory) => {
            println!("capacity: {}", memory.capacity_bytes);
        }
        Config::File(file) => {
            println!("flags: {}", file.flags);
            println!("path: {}", file.path.display());
        }
    }

    Ok(())
}
```

### External-tag variant of the root

```rust
#[derive(Debug, Clone, Copy, ZeroSchema)]
#[zero(endian = "native")]
pub struct ExternalTagMessage<'a> {
    pub sequence: u64,
    pub kind: ConfigKind,

    #[zero(tag_field = kind)]
    pub config: Config<'a>,
}
```

Parsing guarantees:

```rust
match value.config {
    Config::Memory(_) => assert_eq!(value.kind, ConfigKind::Memory),
    Config::File(_) => assert_eq!(value.kind, ConfigKind::File),
}
```

Encoding checks the same relationship and returns an error rather than writing contradictory bytes.

---

# Appendix A: attribute reference

## A.1 Container attributes

```rust
#[zero(
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    padding = "ignore" | "zero",
    validate_with = path::to::function,
    crate = path::to::zero_schema,
    borrow = 'lifetime,
)]
```

For tagged enums:

```rust
#[zero(
    tag = TagEnumType,
    tail = "ignore" | "zero",
)]
```

## A.2 Field attributes

```rust
#[zero(
    capacity = INTEGER,
    len_type = u8 | u16 | u32,
    tail = "ignore" | "zero",
    endian = "native" | "little" | "big",
    align = POWER_OF_TWO,
    tag_field = sibling_identifier,
    validate_with = path::to::function,
    range = RANGE_EXPRESSION,
    must_equal = CONST_EXPRESSION,
)]
```

These options are not freely interchangeable. Section 7.4 is normative; the compact field-applicability summary is:

| Option | Permitted field |
|---|---|
| `capacity` | `&str`, `&CStr`, `&U16Str`, `&U16CStr` |
| `len_type` | `&str`, `&U16Str` |
| `tail` | supported borrowed string fields |
| `endian` | direct numeric or length-prefixed storage, subject to native-endian wide-string rules |
| `align` | any supported field |
| `tag_field` | a tagged-union field with a matching sibling scalar-enum tag |
| `validate_with` | any projected field with a type-correct validator |
| `range` | direct primitive numeric field |
| `must_equal` | direct scalar field |

Unknown, duplicate, contradictory, misplaced, and inapplicable options are compile-time errors. The derive never silently ignores a `#[zero]` option.

## A.3 Variant attributes

```rust
#[zero(tag = TagEnumType::Variant)]
```

## A.4 Defaults

| Setting | Default |
|---|---|
| primitive endian | native |
| length type | `u16` |
| type alignment | natural generated wire alignment |
| field alignment | natural field wire alignment |
| decode padding | ignore |
| decode fixed tail | ignore |
| encode padding | zero |
| encode fixed tail | zero |
| unknown scalar enum | error |
| unknown union tag | error |
| validation | eager |
| tag placement | internal unless `tag_field` is present |

---

# Appendix B: schematic macro expansion

For the input in Appendix A, the 0.1 expansion has the following contract. Exact private identifiers and token ordering are deliberately unspecified.

```rust
// The user's public declaration remains the application type.
pub struct Example<'a> {
    pub id: u32,
    pub name: &'a CStr,
    pub mode: Mode,
}

// A sibling module-scope support module contains a lifetime-free repr(C) Wire,
// concrete non-exhaustive errors, descriptor records, offsets, and assertions.
#[doc(hidden)]
mod __zero_schema_example { /* generated support */ }

impl<'a> zero_schema::ZeroSchemaType for Example<'a> {
    type Wire = __zero_schema_example::Wire;
    type DecodeError = ExampleDecodeError;
    type EncodeError = ExampleEncodeError;
    const WIRE_SIZE: usize = core::mem::size_of::<Self::Wire>();
    const WIRE_ALIGN: usize = core::mem::align_of::<Self::Wire>();
    const WIRE_STRIDE: usize = /* checked size round-up to alignment */;
    const LAYOUT: &'static zero_schema::LayoutDescriptor =
        &__zero_schema_example::LAYOUT;
}

impl<'input, 'a> zero_schema::__private::DecodeWire<'input> for Example<'a>
where
    'input: 'a,
{
    fn decode_at(
        input: zero_schema::__private::DecodeInput<'input, Self::Wire>,
    ) -> Result<Self, Self::DecodeError> {
        // Project fields in declaration/dependency order from checked subranges.
        // Borrowed views come from input.bytes(); nested values receive DecodeInput.
        # unimplemented!()
    }
}

impl<'a> zero_schema::__private::EncodeWire for Example<'a> {
    fn validate_encode(&self) -> Result<(), Self::EncodeError> {
        // Complete semantic preflight in the specified precedence order.
        # unimplemented!()
    }

    fn encode_at(
        &self,
        destination: &mut zero_schema::__private::Prezeroed<'_>,
    ) -> Result<(), Self::EncodeError> {
        // Write only through checked write/subrange capabilities; never re-zero.
        # unimplemented!()
    }
}
```

The inherent `parse` and `parse_prefix` methods construct checked root inputs. `encode_into` constructs the checked destination and delegates once to the composition traits; for a monomorphic or lifetime-only schema, `encode()` creates private `AlignedBytes`, calls `encode_into`, and returns the owned storage only on success. Type- or const-generic schemas instead use `make_buffer_for!(FullyConcreteType)` after monomorphization as specified in Section 9.4. The real expansion uses stable Rust 1.85 syntax, preserves user predicates on public impls, erases lifetimes only from wire/error support, and obtains all layout facts from compiler constants. The pseudocode markers above describe omitted generated bodies; delivered macro output contains no stubs.

---

# Appendix C: source references

The project design relies on the following external guarantees and APIs as of the document date:

1. [`zerocopy` crate documentation, version 0.8.54](https://docs.rs/zerocopy/0.8.54/zerocopy/)
2. [`zerocopy::FromBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.FromBytes.html)
3. [`zerocopy::TryFromBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.TryFromBytes.html)
4. [`zerocopy::IntoBytes`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.IntoBytes.html)
5. [`zerocopy::KnownLayout`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.KnownLayout.html)
6. [`zerocopy::Immutable`](https://docs.rs/zerocopy/0.8.54/zerocopy/trait.Immutable.html)
7. [`zerocopy::byteorder`](https://docs.rs/zerocopy/0.8.54/zerocopy/byteorder/)
8. [Rust Reference: type layout and representations](https://doc.rust-lang.org/reference/type-layout.html)
9. [Rust Reference: procedural macros and derive helper attributes](https://doc.rust-lang.org/reference/procedural-macros.html)
10. [`std::ffi::CStr`](https://doc.rust-lang.org/std/ffi/struct.CStr.html)
11. [`widestring` crate documentation, version 1.2.1](https://docs.rs/widestring/1.2.1/widestring/)
12. [`widestring::U16Str`](https://docs.rs/widestring/1.2.1/widestring/ustr/struct.U16Str.html)
13. [`widestring::U16CStr`](https://docs.rs/widestring/1.2.1/widestring/ucstr/struct.U16CStr.html)

---

## Final design summary

`zero-schema` should present one ordinary, ergonomic Rust schema declaration and generate two coordinated representations:

```text
public validated borrowed type
    native scalars
    &str / &CStr
    &U16Str / &U16CStr
    nested values
    normal Rust enums

hidden fixed-layout wire type
    repr(C)
    raw/endian-aware scalars
    fixed arrays
    generated tag storage
    generated C-layout payload union
```

`zerocopy` establishes that the source bytes can safely be referenced as the hidden wire type. Generated validation then establishes the stronger logical invariants needed to construct the public type. Encoding performs the reverse mapping directly into the caller’s final fixed-size, aligned buffer while deterministically initializing every byte.

The resulting boundary is intended to feel like Serde at the source level, retain C/C++ shared-memory layout control, and keep the application-facing Rust model free from byte-order wrappers, raw buffers, and unsafe union access.
