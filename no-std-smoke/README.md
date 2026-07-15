# `no_std` smoke package

This unpublished workspace package proves that a real `#![no_std]` consumer can use
the producer-byte capability API without feature unification from the root test
harness. It remains separate from the runtime crate because ordinary tests link
`std`, and a broad workspace graph can otherwise enable the runtime's standard
features indirectly.

The package has `publish = false`, shares the workspace lockfile, and depends on:

- `zero-schema` with `default-features = false`;
- direct `zerocopy` with default features disabled and its macro support, as required
  by generated private wire forms.

`#[zero]` is re-exported unconditionally, so it remains available while the smoke
package keeps both `std` and `alloc` disabled.

## Smoke behavior

`src/lib.rs` is unconditionally `#![no_std]`. It declares:

- the closed scalar enum `SmokeKind`;
- a nested `Number` record;
- the logical `Payload` tagged declaration; and
- the root `Packet`, where `kind` is the payload's mandatory external tag.

The reviewed `Packet` bytes live in an explicitly aligned fixture wrapper. The smoke
function calls exact `Packet::access`, reads the scalar tag and selected `Data`
payload, materializes with `copy_into`, then calls `access_mut` and changes only the
selected nested number through a short field-local reborrow. It performs a fresh
access to prove the successful constrained update remains type-valid.

The source does not construct a schema representation from a logical Rust value. Its
constant is a reviewed producer fixture copied into initialized, correctly aligned
Rust test storage. The same application rule applies at a real boundary: the producer
initializes the exact slot, then the consumer asks `access` whether those bytes are
valid for `Packet`.

`src/bin/linked-wasm.rs` is `no_std` and `no_main` only for `wasm32` with
`target_os = "none"`. On that target it supplies a panic handler and exports `_start`,
which invokes the smoke function. On other targets it has an empty `main` solely so
Cargo can discover the target.

## Reproduce the target proofs

Run these commands from the workspace root:

```console
rustup toolchain install 1.85.0 --profile minimal
rustup target add --toolchain 1.85.0 thumbv7em-none-eabihf wasm32v1-none

cargo +1.85.0 check --locked -p zero-schema-no-std-smoke --lib --target thumbv7em-none-eabihf
cargo +1.85.0 build --locked -p zero-schema-no-std-smoke --bin linked-wasm --target wasm32v1-none --release
```

The Thumb command is a **compile proof**: the core-only runtime, `#[zero]` expansion,
and producer-byte capability usage type-check for an embedded target. It does not
link a Thumb executable.

The wasm command is a **link proof**: the freestanding binary, panic handler, `_start`
entry point, generated schemas, and dependencies produce a final optimized
`wasm32v1-none` artifact without a host runtime. Neither command executes target code;
`_start` discards the smoke return value. Together they do not replace the separate
host behavior, Miri, ABI, or fuzz verification layers described in
[`TESTING.md`](../TESTING.md).
