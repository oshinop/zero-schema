# Changelog

All notable changes are documented here. The project follows semantic versioning
for its Rust API; applications must version their wire schemas independently.

## 0.1.0 — 2026-07-13

Initial release.

### Added

- Fixed-layout schemas derived for named records, closed `u8`/`u16`/`u32` scalar
  enums, internally tagged unions, and sibling externally tagged payload fields.
- Borrowed zero-copy decoding for UTF-8, byte C strings, native-wide strings, and
  fixed byte arrays; owned aligned encoding with `encode()`, plus `AlignedBytes`,
  `make_buffer_for!`, and `encode_into` for caller-managed storage.
- Primitive endian codecs, configurable layout alignment, zero/ignore padding and
  tail policies, range/must-equal checks, and typed custom validation contexts.
- Allocation-free structured errors with stable logical paths and optional
  `alloc`-backed path strings.
- The allocation-free structured error model has no `rancor` dependency.
- Public read-only layout descriptor graphs and wire size/alignment/stride constants.
- `no_std` core runtime, optional `alloc`, optional derive facade, and default
  `std + derive` feature set. MSRV is Rust 1.85.0.
- Cross-crate schema composition, property/fuzz/Miri coverage, golden-byte fixtures,
  and unpublished C++17 ABI conformance fixtures and benchmarks.

### Known limitations

- Derives are supported only for module-scope items and require downstream users to
  depend directly on `zerocopy`.
- Type- and const-generic schemas do not expose zero-argument `encode()`; callers
  fully monomorphize the schema and use `make_buffer_for!` with `encode_into`.
- Scalar enums and tagged unions are closed. There are no consumer C++ header
  generator, schema fingerprint, migration protocol, open unions/enums, or
  transactional writer guarantees.
- Native-wide fields interoperate only with matching target endianness and ABI.
