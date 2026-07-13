# Examples

These examples are small, runnable tours of separate parts of `zero-schema`. Read the
source alongside the notes below; the source contains the schema definitions and the
assertions that demonstrate each contract.

## Dependencies for downstream crates

The default `zero-schema` features include `std` and the derive macro. A crate that
derives `ZeroSchema` must also depend directly on `zerocopy` with its derive support;
the generated wire types use those derives.

```toml
[dependencies]
zero-schema = "=0.1.0"
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
```

[`records.rs`](records.rs) also uses the public `widestring` view types, so add:

```toml
widestring = { version = "=1.2.1", default-features = false }
```

For a `no_std` crate, disable `zero-schema`'s defaults and enable only its derive
feature. The direct `zerocopy` dependency remains required:

```toml
[dependencies]
zero-schema = { version = "=0.1.0", default-features = false, features = ["derive"] }
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
```

## Example map

### [`records.rs`](records.rs): records, strings, and borrowed decoding

Shows nested records containing fixed-endian integers, booleans, UTF-8 strings, byte
C strings, wide strings, wide C strings, and fixed byte arrays. It demonstrates exact
and prefix parsing, layout metadata, and that borrowed fields point into the encoded
wire buffer.

```console
cargo run --example records
```

### [`tagged.rs`](tagged.rs): scalar tags and tagged unions

Compares an internally tagged union with an externally tagged payload whose tag lives
in a sibling record field. It also inspects the structured `TagMismatch` error produced
when the external tag disagrees with the selected variant.

```console
cargo run --example tagged
```

### [`validation_errors.rs`](validation_errors.rs): validation and diagnostics

Combines declarative range and equality checks with field-level and whole-schema
validators. It walks layout metadata and demonstrates allocation-free structured error
inspection. Its owned `error_path_string` demonstration requires the `alloc` feature.

```console
cargo run --example validation_errors --no-default-features --features "alloc,derive"
```

### [`generic_buffer.rs`](generic_buffer.rs): aligned storage for a concrete generic schema

Type- and const-generic schemas retain `encode_into` rather than zero-argument
`encode()`. This example fully monomorphizes the schema and uses
`make_buffer_for!(ConcreteType)` to construct safe initialized `AlignedBytes`, then
verifies a zero-copy round trip.

```console
cargo run --example generic_buffer
```

### [`no_std_wasm.rs`](no_std_wasm.rs): freestanding `no_std` linkage

Defines a `no_std`, `no_main` entry point for `wasm32v1-none` and performs an
allocation-free encode/decode round trip. Build it for the target; this command proves
that the example compiles and links, but does not claim to execute the resulting
module:

```console
cargo build --example no_std_wasm --target wasm32v1-none --release --no-default-features --features derive
```

Install that target first if necessary with
`rustup target add wasm32v1-none`.

## Encoded ownership, lifetime, and alignment

For monomorphic and lifetime-only schemas, `encode()` returns owned `AlignedBytes`.
Its type does not borrow from the value being encoded, so borrowed input data need
not outlive the encoded result. The storage owns initialized bytes, satisfies the
wire alignment, exposes exactly `WIRE_SIZE` bytes, and occupies `WIRE_STRIDE` bytes.

Use `make_buffer_for!(FullyConcreteType)` with `encode_into` when demonstrating mutable,
reusable, external, or transactional staging storage, and for a fully monomorphized
type- or const-generic schema. Parsed string and fixed-array fields may borrow from
the encoded bytes, so that storage must remain alive and must not be mutably reused
while a parsed borrow exists.
