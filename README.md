# zero-schema

`zero-schema` declares logical Rust types over an **existing fixed-layout byte
representation**. It is for bytes supplied by a shared-memory peer, device, C/C++
producer, or independently reviewed fixture—not for constructing an arbitrary Rust
value and treating its memory as a protocol.

- Rust MSRV: **1.85.0**.
- Runtime: `#![no_std]`; `alloc` and `std` are optional conveniences.
- Default features: `std`.
- Any crate using `#[zero]` also declares its direct `zerocopy` dependency; any
  `zerocopy` 0.8.x release from 0.8.54 onward is supported.

## Is this the right boundary?

Choose `zero-schema` when a producer and consumer already share a fixed, target ABI
slot and the application can establish its lifetime, initialization, synchronization,
alignment, and layout agreement. It validates that exact slot before logical access
and permits constrained updates of a validated slot.

Choose a serializer or archive such as [`rkyv`](https://rkyv.org/) when the application
owns the data-format boundary or needs a serialized/archive representation rather than
foreign or shared fixed-layout storage. That is a representation and compatibility
choice; this crate does not make a general performance comparison between these
approaches.

## Quick start: run the producer-byte journey

Add the runtime and direct `zerocopy` dependency required by generated private wire forms:

```toml
[dependencies]
zero-schema = "=0.1.0"
zerocopy = { version = "0.8.54", default-features = false, features = ["derive"] }
```

`zerocopy` (and the runtime's `widestring`) are compatible-release requirements, not
exact pins: Cargo resolves the runtime and your direct `zerocopy` dependency to one
release, so a later 0.8.x such as 0.8.61 works. Only the companion
`zero-schema-macros` crate is pinned (`=0.1.0`), so the runtime and its expansion
always move together.

Then run the assertion-driven [`records.rs`](examples/records.rs) example:

```console
cargo +1.85.0 run --locked --example records
```

It starts with independently reviewed producer bytes, proves exact eager access,
reads through a borrowed capability, materializes an explicit logical value, applies
a constrained patch, and proves the result again through fresh access. The runnable
source is the quick start; no schema call is placed at module scope in this README.

## Mental model

Keep these three layers separate:

1. **Logical values** are the `#[zero]` structs and enums your Rust code names,
   pattern-matches, and may explicitly materialize.
2. **Wire representation** is the private fixed-layout storage used to validate and
   access those values. It is not a public raw-wire or mutable-byte API.
3. **Capabilities** are generated for each root. For a root named `Record`,
   `Record::access` yields a shared `RecordRef<'_>` for zero-copy reads, while
   `Record::access_mut` yields an exclusive `RecordMut<'_>` for constrained mutation.
   Borrowed outputs remain tied to the producer storage.

Those constructors validate one exact supplied span before producing a capability. When
the span you hold is longer than the schema (for example a union-sized payload span),
`Record::access_prefix` and `Record::access_mut_prefix` prove exactly the leading
`SCHEMA_SIZE` bytes with the same alignment rule and eager proof and return the
remainder untouched. If a producer needs Rust-owned receiving memory,
`schema_buffer!(Root)` names its exact aligned buffer type and
`make_schema_buffer!(Root)` constructs an initialized value; with the `alloc` feature,
`make_boxed_schema_buffer!(Root)` builds the same zero-filled buffer directly on the
heap as a `Box`, with no stack temporary, for large roots. All of them require a fully
concrete root. Those initial bytes are **not** a schema initializer: let the producer
fill the slot, then call `access`. A root whose all-zero bytes are already a valid
instance can say so instead; see the zero-start constructors below.

```rust
# use zero_schema::zero;
# #[zero]
# struct Message { value: u32 }
# fn receive_producer_bytes(bytes: &mut [u8]) { bytes.copy_from_slice(&7_u32.to_ne_bytes()); }
# fn main() {
type MessageBuffer = zero_schema::schema_buffer!(Message);
let mut slot: MessageBuffer = zero_schema::make_schema_buffer!(Message);
receive_producer_bytes(slot.as_bytes_mut());
let message = Message::access(slot.as_bytes()).unwrap();
assert_eq!(message.value(), 7);
# }
```

### Starting from zero, checked at compile time

Every generated record and scalar enum has a `const ZERO_IS_VALID: bool`: exactly
whether an all-zero span is accepted by `access`. Primitives, `bool`, fixed bytes, every
bounded string form, and `Option<T>` are always zero-valid; a scalar enum needs a
`= 0` variant; nested schemas and fixed arrays take their element's value; a tagged
union needs the variant selected by tag `0` to be zero-valid.
`make_zeroed_schema_buffer!(Root)` (and, with `alloc`,
`make_zeroed_boxed_schema_buffer!(Root)`) require that constant at compile time and
otherwise build the same buffer as `make_schema_buffer!`, which is type-valid until its
bytes are changed. `access` and `access_mut` stay the only way to get a capability:
they still prove the zero bytes, and there is no unchecked variant.

```rust
# use zero_schema::zero;
# #[zero]
# #[repr(u8)]
# #[derive(Clone, Copy, Debug, Eq, PartialEq)]
# enum Mode { Idle = 0, Busy = 1 }
# #[zero]
# struct Status { mode: Mode, count: u32 }
# fn main() {
const { assert!(Status::ZERO_IS_VALID) };
let slot = zero_schema::make_zeroed_schema_buffer!(Status);
let status = Status::access(slot.as_bytes()).unwrap();
assert_eq!((status.mode(), status.count()), (Mode::Idle, 0));
# }
```

An enum without a `= 0` discriminant makes its root zero-invalid, and the macro then
fails to compile with a message naming `ZERO_IS_VALID`. Declare a zero variant, or make the
field `Option<Enum>`, whose all-zero span is `None`. For a generic root, the constant
depends on its type arguments, so name it with every parameter supplied.

```rust
# use zero_schema::zero;
# #[zero]
# struct Message { value: u32 }
# fn main() {
#[repr(align(4))]
struct Span([u8; 8]);
let mut span = Span([0; 8]);
span.0[..4].copy_from_slice(&7_u32.to_ne_bytes());
let (message, rest) = Message::access_prefix(&span.0).unwrap();
assert_eq!((message.value(), rest.len()), (7, 4));
# }
```

### Read and write direction

- `view.copy_into()` moves a checked wire capability **into** a logical Rust value.
- `view_mut.copy_from(&patch)` moves a logical patch **from** the caller **into** an
  already checked mutable capability.

Field mutation is intentionally short-lived: each `*_mut()` returns a field-local
reborrow, so sequential updates are possible without handing out a mutable aggregate
or its raw storage. See [`SAFETY.md`](SAFETY.md) for the borrowing, receiving-storage,
and mutation invariants.

A bounded-string handle (`*_mut()` on a `str`, `CStr`, `U16Str`, or `U16CStr` field, or an
element of a string array) also
reports its declared `#[zero(capacity = N)]` through read-only `capacity()`: bytes for
`str` and `CStr`, `u16` units for the wide forms, counting the terminator for the C forms
and excluding the length prefix for the prefixed forms. It is metadata only; `set`
rejects an overlong value and no setter truncates.

### Three representation rules worth finding early

- **`Option<T>` is a zero sentinel.** There is no presence byte: `None` is an all-zero
  complete field span, and only types for which zero cannot be a valid present value
  are eligible. The [`optional.rs`](examples/optional.rs) tour shows the resulting
  `OptionMut` and patch behavior; the complete rule is in the
  [design RFC](docs/zero-schema-design-rfc.md) and [SAFETY.md](SAFETY.md).
- **Tagged payloads have an external tag.** A root record owns the scalar tag and its
  payload together. A tagged payload is not an independent root, and mutation cannot
  change the tag separately from the payload. The [`tagged.rs`](examples/tagged.rs)
  tour demonstrates the coupled switch; the normative model is in the
  [design RFC](docs/zero-schema-design-rfc.md).
- **Arrays may hold bounded strings or other arrays.** `[&U16CStr; 4]` with
  `#[zero(capacity = 49)]` is the C++ `wchar_t names[4][49]` (16-bit `wchar_t`), and
  `[[u16; 49]; 4]` is `uint16_t grid[4][49]`. Strings stay zero-copy per element,
  a row is an `ArrayRef`/`ArrayMut` view, and errors name every index (`names[2]`,
  `grid[1][3]`). See the [design RFC §6.7](docs/zero-schema-design-rfc.md#67-fixed-arrays).

## Follow the runnable examples

Work through these assertion-driven examples in order. They use reviewed producer
bytes or receiving storage rather than encoding test fixtures through generated APIs.

| Step | Example | Feature and invariant demonstrated | Command and required features |
| --- | --- | --- | --- |
| 1 | [`records.rs`](examples/records.rs) | Producer-owned exact bytes are eagerly proven before `Ref` getters; `copy_into` and patch `copy_from` have opposite directions; a wrong-length array `copy_from` leaves the complete slot unchanged. | `cargo +1.85.0 run --locked --example records` (default `std`) |
| 2 | [`strings.rs`](examples/strings.rs) | Bounded borrowed UTF-8, C, and wide strings plus fixed bytes preserve inactive capacity; rejected constrained writes preserve the destination. | `cargo +1.85.0 run --locked --example strings` (default `std`) |
| 3 | [`tagged.rs`](examples/tagged.rs) | Short reborrows select an externally tagged payload; a tag-only patch is rejected without changing the coupled tag or payload bytes. | `cargo +1.85.0 run --locked --example tagged` (default `std`) |
| 4 | [`optional.rs`](examples/optional.rs) | An eligible `Option<T>` uses a complete all-zero field span for absence; `OptionMut` and tri-state patches preserve that invariant. | `cargo +1.85.0 run --locked --example optional` (default `std`) |
| 5 | [`access_errors.rs`](examples/access_errors.rs) | Malformed producer Boolean and enum values are rejected before a capability exists; owned diagnostics are explicitly `alloc`-gated. | `cargo +1.85.0 run --locked --example access_errors --no-default-features --features alloc` (`alloc`) |
| 6 | [`generic_receiving_buffer.rs`](examples/generic_receiving_buffer.rs) | A fully concrete generic root gets correctly aligned receiving storage; `SCHEMA_*` and `LAYOUT` report its diagnostic ABI facts. | `cargo +1.85.0 run --locked --example generic_receiving_buffer` (default `std`) |
| 7 | [`no_std_wasm.rs`](examples/no_std_wasm.rs) | The same eager producer-byte access works in a freestanding, core-only `wasm32v1-none` build. | `rustup target add --toolchain 1.85.0 wasm32v1-none` then `cargo +1.85.0 build --locked --example no_std_wasm --target wasm32v1-none --release --no-default-features` (no runtime features) |

The `access_errors` command enables `alloc` only because that diagnostic example uses
owned formatting. The access proof, zero-copy reads, constrained mutation, and the
other tours do not need it. [`examples/README.md`](examples/README.md) is a
topic-indexed companion rather than this staged learning path.

For an ABI review, generated roots expose `SCHEMA_SIZE`, `SCHEMA_ALIGN`,
`SCHEMA_STRIDE`, and diagnostic `LAYOUT` metadata;
[`records.rs`](examples/records.rs) and
[`generic_receiving_buffer.rs`](examples/generic_receiving_buffer.rs) assert the
layout facts. A C/C++ producer still needs an agreed target ABI and reviewed fixtures. The
[`TESTING.md` C++ conformance section](TESTING.md#c-conformance-and-reviewed-fixtures)
covers the cross-language evidence; the [design RFC](docs/zero-schema-design-rfc.md)
defines the compatibility contract.

## Features and `no_std`

| Feature | Purpose |
| --- | --- |
| `alloc` | Enables owned error-path formatting and heap receiving storage (`SchemaBuffer::new_boxed`, `make_boxed_schema_buffer!`, `make_zeroed_boxed_schema_buffer!`). |
| `std` | Enables `alloc` and `zerocopy` standard-library integration. |

For a `no_std` schema crate, disable defaults. `#[zero]` is re-exported
unconditionally, so this keeps both `std` and `alloc` disabled:

```toml
[dependencies]
zero-schema = { version = "=0.1.0", default-features = false }
zerocopy = { version = "0.8.54", default-features = false, features = ["derive"] }
```

The [`no_std_wasm.rs`](examples/no_std_wasm.rs) command above is a compile/link proof,
not host execution. [`no-std-smoke`](no-std-smoke/README.md) contains the additional
Thumb and freestanding wasm checks.

## More detail and verification

The [normative design RFC](docs/zero-schema-design-rfc.md) defines the wire model,
accepted declarations, layout, eager proof, patch semantics, external tagging, ABI
scope, and evolution rules. This README is a guide to choosing and trying the crate;
the RFC resolves normative questions.

- [`SAFETY.md`](SAFETY.md): producer responsibilities and Rust safety invariants.
- [`examples/README.md`](examples/README.md): walkthroughs for every runnable example.
- [`TESTING.md`](TESTING.md): focused, feature-matrix, target, and ABI verification.
- [`CHANGELOG.md`](CHANGELOG.md): released public-contract changes.
