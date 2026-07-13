# `no_std` smoke package

This unpublished workspace package verifies that generated `zero-schema` code remains usable by a real `#![no_std]` consumer. It is separate from the runtime crate even though the runtime itself is `no_std` because ordinary unit and integration tests are not sufficient evidence:

- Rust's test harness links `std`, so a test can compile despite accidental standard-library use.
- Cargo feature unification can enable `zero-schema/std` through another workspace dependency, especially in `--all-features` test runs.
- Cross-compiling exposes dependencies or generated code that only work on the host target.
- Compiling a library does not prove that a complete freestanding executable can link without a host runtime.

The package is `publish = false`, shares the workspace lockfile, and has no feature forwarding of its own. Its manifest depends on:

- `zero-schema` with `default-features = false` and only `features = ["derive"]`;
- `widestring` with default features disabled, for borrowed UTF-16 schema fields;
- `zerocopy` with default features disabled and only its derive support, as required by generated derives.

## Smoke code

`src/lib.rs` is unconditionally `#![no_std]`. It derives and exercises:

- a scalar tag enum;
- borrowed UTF-8, C string, UTF-16, UTF-16 C string, and fixed-byte fields;
- a nested record;
- unit and data variants of an internally tagged union;
- owned `AlignedBytes`, `make_buffer_for!`, encoding, exact parse, and prefix parse.

`smoke_roundtrip()` returns `0` after encoding and parsing the borrowed record and nested tagged payload; nonzero values identify a failed stage. `smoke_prefix()` returns `0` when prefix parsing consumes exactly `Packet::WIRE_SIZE` and leaves the three trailing bytes intact.

`src/bin/linked-wasm.rs` becomes `no_std` and `no_main` only on `wasm32` with `target_os = "none"`. For that target it supplies a panic handler and exports `_start`, which calls both smoke functions. On host targets it has an empty `main` so normal workspace target discovery still succeeds.

## Reproduce the target proofs

Run these commands from the workspace root:

```console
rustup toolchain install 1.85.0 --profile minimal
rustup target add --toolchain 1.85.0 thumbv7em-none-eabihf wasm32v1-none

cargo +1.85.0 check --locked -p zero-schema-no-std-smoke --lib --target thumbv7em-none-eabihf
cargo +1.85.0 build --locked -p zero-schema-no-std-smoke --bin linked-wasm --target wasm32v1-none --release
```

The Thumb command is a **compile proof**: the `#![no_std]` library, generated schemas, and core-compatible dependencies type-check for an embedded target. It does not link a Thumb executable.

The wasm command is a **link proof**: the freestanding binary, panic handler, `_start` entry point, generated code, and dependencies produce a final optimized `wasm32v1-none` artifact without a standard host runtime.

Neither command is a **runtime execution proof**. The artifacts are not run, `_start` discards the smoke return codes, and target runtime behavior is therefore not observed. These target checks cover the runtime's core configuration plus `derive`; they do not separately prove core without derive, `alloc`, or `std` feature modes. The wasm build is the only complete freestanding executable link in this package.