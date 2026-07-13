# zero-schema

`zero-schema` defines fixed-layout Rust wire schemas for shared memory and C++
interoperability. Decoding borrows strings and fixed byte arrays directly from an
aligned input buffer; encoding normally returns owned, correctly aligned storage.
The wire format is described by generated metadata and uses no Rust references,
`bool`, or Rust enum layout.

The minimum supported Rust version (MSRV) is **1.85.0**. Version 0.1 uses schema
format version **0.1**; the crate version is not an automatic on-wire version.

## Quick start

With the default features enabled:

```rust
use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
struct Greeting<'a> {
    sequence: u32,
    #[zero(capacity = 12, len_type = u8, tail = "zero")]
    text: &'a str,
}

let value = Greeting { sequence: 7, text: "hello" };
let storage = value.encode().unwrap();

let decoded = Greeting::parse(storage.as_bytes()).unwrap();
assert_eq!(decoded, value);
assert_eq!(decoded.encoded_len(), Greeting::WIRE_SIZE);
```

Borrowed values in `decoded` point into `storage`; the storage must therefore outlive
them. The returned `AlignedBytes` owns initialized wire bytes, is aligned for the
schema wire, exposes exactly `WIRE_SIZE` bytes, and occupies `WIRE_STRIDE` bytes.
`parse_prefix` consumes exactly `WIRE_SIZE` bytes and returns the remainder.

## Examples

The [`examples/`](examples/README.md) directory contains five complete, runnable
tours rather than disconnected snippets:

- [`records.rs`](examples/records.rs): nested records, every borrowed string form,
  fixed bytes, exact/prefix parsing, metadata, and zero-copy borrow checks;
- [`tagged.rs`](examples/tagged.rs): scalar enums plus internal and external tags;
- [`validation_errors.rs`](examples/validation_errors.rs): declarative and custom
  validation, structured errors, owned paths, and layout inspection;
- [`generic_buffer.rs`](examples/generic_buffer.rs): safe aligned storage for a
  concrete generic schema;
- [`no_std_wasm.rs`](examples/no_std_wasm.rs): an allocator-free freestanding wasm
  link example.

Run a host example with `cargo run --example records`. The [examples guide](examples/README.md)
lists every command, feature requirement, and downstream dependency.

## Schema families

### Records

Named-field structs may contain primitives, `bool`, nested schemas,
`&[u8; N]`, `&str`, `&core::ffi::CStr`, `&widestring::U16Str`, and
`&widestring::U16CStr`.

```rust
use zero_schema::ZeroSchema;
# mod schema {
# use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
struct Header { version: u16, flags: u32 }

#[derive(Debug, PartialEq, ZeroSchema)]
struct Packet<'a> {
    header: Header,
    #[zero(capacity = 32)]
    name: &'a str,
    digest: &'a [u8; 4],
}
# }
```

### Scalar enums

A scalar enum is fieldless, has explicit discriminants, and has exactly one
`#[repr(u8)]`, `#[repr(u16)]`, or `#[repr(u32)]`. Unknown values are errors.

```rust
use zero_schema::ZeroSchema;
# mod schema {
# use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "big")]
enum Status { Ready = 1, Busy = 2 }
# }
```

### Internally tagged unions

Tagged enums have at least one unit or newtype variant. Their tag type is a scalar
enum; each public variant maps to one tag variant. Both the scalar tag domain and
the union are closed.

```rust
use zero_schema::ZeroSchema;
# mod schema {
# use zero_schema::ZeroSchema;

#[derive(ZeroSchema)]
#[repr(u8)]
enum Kind { Empty = 0, Number = 1 }
#[derive(Debug, PartialEq, ZeroSchema)]
struct Number { value: u32 }
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Kind, tail = "zero")]
enum Message {
    #[zero(tag = Kind::Empty)] Empty,
    #[zero(tag = Kind::Number)] Number(Number),
}
# }
```

### Externally tagged fields

A record may store the scalar tag separately. `tag_field` names the sibling tag;
encoding rejects a tag that does not match the selected payload variant.

```rust
# use zero_schema::ZeroSchema;
# mod schema {
# use zero_schema::ZeroSchema;
# #[derive(ZeroSchema)] #[repr(u8)] enum Kind { Empty = 0 }
# #[derive(ZeroSchema)] #[zero(tag = Kind)] enum Message { #[zero(tag = Kind::Empty)] Empty }
#[derive(ZeroSchema)]
struct Envelope {
    kind: Kind,
    #[zero(tag_field = kind)]
    message: Message,
}
# }
```

## Attributes and defaults

Attributes use `#[zero(key = value, ...)]`:

- Struct: `endian = "native" | "little" | "big"`, `align = N`,
  `padding = "ignore" | "zero"`, `validate_with = path`, `borrow = 'a`, and
  `crate = path`.
- Scalar enum: `endian` and `crate`.
- Tagged enum: required `tag = Path`, plus `tail`, `align`, `padding`,
  `validate_with`, `borrow`, and `crate`. Each variant requires
  `#[zero(tag = TagPath::Variant)]`.
- Fields: `endian`, `align`, `capacity = N`, `len_type = u8 | u16 | u32`,
  `tail`, `tag_field = sibling`, `validate_with = path`, `range = start..end`
  (or `..=`), and `must_equal = expression`, where applicable to that field kind.

Numeric and length endianness defaults to native. Length-prefixed strings default
to `u16`. Generated alignment is natural. Decode ignores padding, string tails, and
inactive union storage by default; `"zero"` rejects nonzero bytes. Encoding always
starts with zeroed root storage, so unused padding and tails are zero. Validation is
eager. Enums and unions are closed.

`capacity` is required for all string fields. It counts bytes for `str`/`CStr` and
16-bit code units for wide strings; C forms include the terminator in required
encoding capacity and cannot have capacity zero. `len_type` applies only to `str`
and `U16Str`. Wide string storage is native-endian: requesting explicit little or
big endian is accepted only on a target with that native endianness.

Validators have the public form `fn(&Value, &ValidationContext<'_>) ->
ValidationResult`. They must be total and nonpanicking for every projected value if
arbitrary-byte decoding is expected not to panic. Allocations, side effects, and
panics inside user validators are outside the runtime guarantees.

## Generated public API

Each schema exposes `parse`, `parse_prefix`, `encode_into`, `encoded_len`,
`WIRE_SIZE`, `WIRE_ALIGN`, `WIRE_STRIDE`, and `LAYOUT`. Schemas without type or const
parameters—including lifetime-only schemas—also expose zero-argument `encode()`,
which returns owned `AlignedBytes` and does not borrow from the encoded value.

Use `make_buffer_for!(FullyConcreteType)` to create zeroed `AlignedBytes` when encoding
into reusable or explicitly staged storage with `encode_into`. The macro also works
after fully monomorphizing a type- or const-generic schema; stable Rust cannot put a
type-dependent byte-array length in a generated generic storage type, so no such
schema-named type exists. `AlignedBytes<W, N>` provides `zeroed`, `as_bytes`,
`as_bytes_mut`, `AsRef<[u8]>`, and `AsMut<[u8]>`; its owned byte view has length
`WIRE_SIZE`, its address has `WIRE_ALIGN`, and its value size is `WIRE_STRIDE`.

`LAYOUT` is a read-only `LayoutDescriptor` graph describing names, kinds, offsets,
sizes, alignment, padding, fields, enum values, and variants. Treat layout identity
structurally, not by pointer identity.

Decode and encode errors implement `SchemaError` and `core::error::Error`. They
report an allocation-free `ErrorKind`, schema, optional field/variant segment,
child error, and optional validation code. With `alloc`, `error_path_string`
constructs the logical path. `Display` is stable as
`Schema[.field|.Variant...]: leaf message`; match non-exhaustive error enums with a
wildcard.

## Features and allocation

| Feature | Effect |
|---|---|
| `derive` | Re-exports `ZeroSchema`; optional dependency on `zero-schema-derive`. |
| `alloc` | Enables owned `error_path_string`; codec operations remain allocation-free. |
| `std` | Enables `alloc` and zerocopy's standard-library integration. |
| default | `std` + `derive`. |

The runtime is always `#![no_std]`. `--no-default-features` supplies the core
runtime; add `derive` for macros or `alloc` for owned error paths. Generated
parse/encode/error paths allocate nothing. Caller-owned containers, formatting into
owned strings, and user validators may allocate. Encoding copies primitive bytes,
length prefixes, string contents, and fixed arrays into the destination; decoding
projects primitives by value but borrows string and fixed-array payloads without
copying. This is zero-copy borrowed decoding, not a claim that encoding performs no
copies.

The derive crate must be able to name both the runtime and `zerocopy`. Downstream
crates using the derive therefore need a **direct dependency on `zerocopy`**, even
when `zero-schema` is renamed or `#[zero(crate = path)]` is used. Derives are
supported only on module-scope items, not function/block-local items. A public
generic schema containing a private child can conflict with crate-level
`forbid(private_bounds)` or `forbid(warnings)`; make the child sufficiently visible.
Re-exporting a public schema from a private module may likewise fail Rust privacy
checks. These are Rust visibility limitations, not wire-format fallbacks.

### Why `no-std-smoke` is a separate crate

The runtime declaring `#![no_std]` is necessary but not sufficient evidence. Rust's
test harness links `std`, and Cargo feature unification in a broad workspace test can
silently enable `zero-schema/std`. The unpublished [`no-std-smoke`](no-std-smoke/README.md)
consumer therefore disables default features and enables only `derive`. CI compiles
its library for `thumbv7em-none-eabihf` and links a `no_std`/`no_main` executable for
`wasm32v1-none`. The first is a cross-target compile proof and the second is a final
freestanding link proof; neither target artifact is executed.

## Encoding failure and publication

`encode()` is the normal path when an owned result is wanted: an error drops its
private output, so no invalid bytes can be observed. Use `encode_into` for mutation,
external storage, explicit staging, or fully concrete generic schemas. It checks
destination size/alignment and performs semantic preflight before normal writing.
Semantic failures currently preserve the destination, but the public guarantee is
deliberately weaker: after layout checks pass, **any returned error invalidates the
destination contents**. Do not publish or share it until encoding returns `Ok(())`;
use a separate inactive `make_buffer_for!` value when a transactional update is required.

## ABI and interoperability

Wire compatibility requires the same target ABI/data model and endian profile,
C++17 exact-width integer types, and default non-packed layout. Do not use C++
`bool`, C++ enum storage, `wchar_t`, packing pragmas, bitfields, pointer punning, or
ABI-changing compiler flags for mirror types.

The repository's CI declares conformance profiles for Linux x86_64 with GCC and
Clang, Linux i686 with GCC, macOS arm64 with AppleClang, Windows x86_64 with MSVC,
and big-endian Linux powerpc64 with GCC under QEMU. A local build proves only its
current host/compiler pair. All-unit payloads have no standalone C++ `sizeof`
claim; only their enclosing generated layout is compared.

This crate does **not** generate consumer C++ headers and has no schema fingerprint
or migration protocol. The repository's C++ mirrors are unpublished conformance
test fixtures, not a public generator.

## Evolution rules

Persist an application schema/version field and compare it before interpreting
messages. Changing field order, type, endian, capacity, alignment, padding/tail
policy, enum representation/discriminant, tag mapping, or payload shape can change
layout or meaning. Always compare `WIRE_SIZE` and the complete descriptor/fixtures
when evolving a protocol. Adding a scalar-enum value or tagged-union variant is a
breaking decoding change for older closed readers. Version 0.1 provides no open
enums/unions, fingerprint negotiation, automatic migration, or transactional
writer.

For a complete map of unit, integration, UI, property, Miri, fuzz, conformance,
cross-target, automation, and benchmark coverage, see [TESTING.md](TESTING.md).

See [SAFETY.md](SAFETY.md) for the safety case and [CHANGELOG.md](CHANGELOG.md) for
release history.
