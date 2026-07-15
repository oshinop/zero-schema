# Executable application tours

These examples are short, assertion-driven application journeys for `0.1.0`. Each
one starts with independently reviewed, explicitly aligned producer bytes; Rust
proves exact eager access, borrows logical data, performs only constrained updates,
and checks a fresh access afterward. The fixtures are explanatory input, not a
general-purpose byte producer.

Run an individual command from the repository root. This page is the executable-tour
map, not the normative specification or the maintainer verification command index.

## Choose a tour

Follow these in order when learning the API. Every linked source is a registered
Cargo example.

### 1. Records and access — [`records.rs`](records.rs)

The first tour asserts `Record::LAYOUT`, the aligned producer record's nested Boolean,
array, and borrowed token values, then materializes one logical copy. It exercises
`ArrayMut`, a partial record patch, and a wrong-length array copy that returns
`ArrayLengthMismatch` while preserving the complete producer byte span. A fresh root
view must observe sequence `9`, `active == false`, and samples `[19, 21, 23]`.

**Expected:** `record sequence=9 samples=[19, 21, 23]` after the assertions pass.

```console
cargo +1.85.0 run --locked --example records
```

### 2. Strings and endian fields — [`strings.rs`](strings.rs)

This tour asserts four borrowed string forms, a borrowed fixed-byte array, an explicitly
big-endian scalar, and preservation of inactive string capacity and padding. It then
proves that an over-capacity update fails byte-for-byte without changing the producer
fixture.

**Expected:** `borrowed strings: text=ox c_text=[81] wide_units=1 fixed_bytes=4`.

```console
cargo +1.85.0 run --locked --example strings
```

### 3. Tagged unions — [`tagged.rs`](tagged.rs)

The initial view asserts that `Reading` selects its payload and leaves `Control`
unavailable. A field update keeps that selection; a tag-only patch returns
`TagOnlyPatch` and preserves the complete producer byte span; then a complete patch
switches both observable tag views to `Control` with `(level, enabled) == (5, true)`.

**Expected:** `tagged kind=Control control.level=5`.

```console
cargo +1.85.0 run --locked --example tagged
```

### 4. Optional sentinels — [`optional.rs`](optional.rs)

Reviewed all-zero producer bytes first assert absent scalar, schema, and array option
fields. The tour sets and reborrows all three, clears the scalar's complete field span
without touching parent padding, rejects an incomplete promotion byte-for-byte, then
checks complete/retain/clear patches and a final `From<Settings>` update.

**Expected:** `optional scalar, schema, and array fields exercised transactionally`.

```console
cargo +1.85.0 run --locked --example optional
```

### 5. Structured errors and scalar roots — [`access_errors.rs`](access_errors.rs)

The scalar root first asserts successful read, explicit copy, and constrained mutation
of `Mode::Ready`; an unknown scalar value then yields a root `UnknownEnumValue` at
`Mode`. Malformed records separately assert `InvalidBool` at `Status.active` and
`UnknownEnumValue` at `Status.mode`, including the latter's leaf cause and formatted
paths.

**Expected:** three rejection lines: the scalar root, invalid Boolean, and unknown
record enum.

```console
cargo +1.85.0 run --locked --example access_errors --no-default-features --features alloc
```

This is the one diagnostics tour that enables `alloc`, because it exercises formatted
error paths. Its schema declaration still requires the direct downstream dependency
shown below.

### 6. Generic receiving storage — [`generic_receiving_buffer.rs`](generic_receiving_buffer.rs)

After fully instantiating its lifetime/const-generic root, this tour asserts
`FourBytes::LAYOUT` for the concrete root and its fixed-byte field, plus the receiving
buffer's byte length, size, and alignment. Only after copying producer bytes does fresh
access assert the four borrowed bytes.

**Expected:** `generic receiving buffer has 4 producer bytes`.

```console
cargo +1.85.0 run --locked --example generic_receiving_buffer
```

### 7. `no_std` target — [`no_std_wasm.rs`](no_std_wasm.rs)

The freestanding entry point asserts that an exact producer record selects `Data` and
borrows the little-endian number `7`; it has neither a host runtime nor console output.

**Expected:** successful compile and link for `wasm32v1-none`; the module is not run by
this command.

```console
rustup target add --toolchain 1.85.0 wasm32v1-none
cargo +1.85.0 build --locked --example no_std_wasm --target wasm32v1-none --release \
  --no-default-features
```

The cross-target command intentionally disables defaults. `#[zero]` is re-exported
unconditionally, so it remains available without `std` or `alloc`.

## Metadata and native conformance boundary

The records and generic tours expose generated `LAYOUT` metadata for concrete Rust
schemas. Native C++ interoperability is verified separately through the
[`conformance` FFI boundary](../conformance/src/ffi.rs) and
[`conformance/src/tests`](../conformance/src/tests/); those are conformance sources and
tests, not an executable application tour or a C++ tutorial.

## Downstream dependencies

The default runtime feature is `std`. A downstream crate that declares schemas needs
its own direct `zerocopy` dependency because generated private wire forms resolve it
in the consuming crate:

```toml
[dependencies]
zero-schema = "=0.1.0"
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
```

For a core-only consumer, disable runtime defaults. `#[zero]` remains available
because it is re-exported unconditionally:

```toml
[dependencies]
zero-schema = { version = "=0.1.0", default-features = false }
zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
```

## Where to go next

These application tours demonstrate observable behavior; they do not replace the
documents below.

- [Root README](../README.md): declaration grammar, generated API surface, and the
  broad user-facing introduction.
- [SAFETY.md](../SAFETY.md): caller responsibilities, producer-byte preconditions,
  and memory-safety invariants.
- [TESTING.md](../TESTING.md): the maintainer verification matrix and its complete
  command inventory.
- [Design RFC](../docs/zero-schema-design-rfc.md): the normative representation and
  API contract.

For a larger core-only consumer and additional cross-target context, see the
[`no-std-smoke`](../no-std-smoke/README.md) package.
