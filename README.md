# zero-schema

`zero-schema` declares logical Rust types over an **existing fixed-layout byte
representation**. It is for bytes supplied by a shared-memory peer, device, C/C++
producer, or independently reviewed fixture—not for constructing an arbitrary Rust
value and treating its memory as a protocol.

- Rust MSRV: **1.85.0**.
- Runtime: `#![no_std]`; `alloc` and `std` are optional conveniences.
- Default features: `std`.
- Any crate using `#[zero]` also declares its direct `zerocopy` dependency.

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
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
```

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

Those constructors validate one exact supplied span before producing a capability. If
a producer needs Rust-owned receiving memory, `schema_buffer!` accepts a fully
concrete root and supplies aligned, initialized storage. Its initial bytes are **not**
a schema initializer: let the producer fill the slot, then call `access`.

### Read and write direction

- `view.copy_into()` moves a checked wire capability **into** a logical Rust value.
- `view_mut.copy_from(&patch)` moves a logical patch **from** the caller **into** an
  already checked mutable capability.

Field mutation is intentionally short-lived: each `*_mut()` returns a field-local
reborrow, so sequential updates are possible without handing out a mutable aggregate
or its raw storage. See [`SAFETY.md`](SAFETY.md) for the borrowing, receiving-storage,
and mutation invariants.

### Two representation rules worth finding early

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
`SCHEMA_STRIDE`, and diagnostic `LAYOUT` metadata; [`records.rs`](examples/records.rs)
and [`generic_receiving_buffer.rs`](examples/generic_receiving_buffer.rs) assert those
facts. A C/C++ producer still needs an agreed target ABI and reviewed fixtures. The
[`TESTING.md` C++ conformance section](TESTING.md#c-conformance-and-reviewed-fixtures)
covers the cross-language evidence; the [design RFC](docs/zero-schema-design-rfc.md)
defines the compatibility contract.

## Features and `no_std`

| Feature | Purpose |
| --- | --- |
| `alloc` | Enables owned error-path formatting. |
| `std` | Enables `alloc` and `zerocopy` standard-library integration. |

For a `no_std` schema crate, disable defaults. `#[zero]` is re-exported
unconditionally, so this keeps both `std` and `alloc` disabled:

```toml
[dependencies]
zero-schema = { version = "=0.1.0", default-features = false }
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
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
