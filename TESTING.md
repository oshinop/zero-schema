# Testing zero-schema

This document is the maintainer map for the repository's verification layers. It describes what each test-bearing file or fixture proves, why non-`#[test]` inputs exist, and the command that executes it. The layers are deliberately complementary: unit tests pin runtime primitives, integration tests exercise generated public APIs, UI tests pin diagnostics, target fixtures prove compilation properties, and conformance/fuzz/Miri gates cover risks that ordinary host tests cannot.

Commands below run from the repository root unless stated otherwise. CI uses `--locked`; local focused commands should do the same. Rust 1.85.0 is the MSRV, `stable` is the moving compatibility toolchain, 1.97.0 is the pinned CI/tooling toolchain, and `nightly-2025-11-15` is the pinned Miri/fuzz toolchain.

## Fast entry points

```sh
# Runtime inline units and all root integration targets
cargo +1.85.0 test --locked -p zero-schema --lib
cargo +1.85.0 test --locked -p zero-schema --tests

# Workspace behavioral gate (derive UI is separate)
cargo +1.85.0 test --locked --workspace --all-features \
  --exclude zero-schema-derive --lib --tests
cargo +1.85.0 test --locked -p zero-schema-derive --lib
cargo +1.85.0 test --locked -p zero-schema-derive --test ui

# One root integration target
cargo +1.85.0 test --locked -p zero-schema --test roundtrip
```

The same workspace behavioral commands run on moving `stable`. Feature compilation, docs, target checks, C++ compiler matrices, Miri, fuzzing, and ignored tests remain separate gates; a green default `cargo test` does not replace them.

## Runtime inline unit modules

`cargo +1.85.0 test --locked -p zero-schema --lib` runs the eight inline modules:

| File | Contract protected |
| --- | --- |
| `src/codec.rs` | UTF-8 length, capacity, invalid-data and zero-tail rules; C first-NUL, missing-NUL and tail rules; U16 code-unit lengths, unpaired surrogates and tails; confined writes. |
| `src/decode.rs` | Exact/prefix size and alignment precedence, checked subrange overflow and bounds, reusable `DecodeInput`, exact remainders, and retention of original bytes for padding inspection. |
| `src/encode.rs` | A single root zero-fill, confined subranges, overflow precedence, active-byte copy counters, and panic-safe restoration of test instrumentation. |
| `src/error.rs` | Stable `LayoutError` text, nested allocation-free `SchemaError` paths, deepest-leaf formatting, and the alloc-gated owned path helper. |
| `src/layout.rs` | Constructors and getters for every descriptor and policy type. `ByteRange` construction is intentionally nonvalidating because generated const assertions own range validity. |
| `src/schema.rs` | Scalar enum codecs and domains, checked stride computation including bad alignment/overflow, generic tagged projections, and lifetime-separated decode/encode composition. |
| `src/validation.rs` | Field/whole `ValidationContext` contents and `ValidationFailure` code, message, and display behavior. |
| `src/wire.rs` | Native/little/big integer and float bit codecs (including NaN and signed zero), bool domain, scalar representations, and helper sizes/offsets. |

## Root integration targets

Cargo auto-discovers the 29 files directly under `tests/`. Run any one with `cargo +1.85.0 test --locked -p zero-schema --test <stem>`.

| Target | Contract protected |
| --- | --- |
| `alignment.rs` | `AlignedBytes`, fully concrete `make_buffer_for!` storage, size/alignment/stride, misaligned parse/encode, untouched destinations, and short-size precedence. |
| `allocation.rs` | Ordinary, non-ignored allocator liveness and zero-allocation generated parse, encode, validation, and error paths. |
| `buffer_support.rs` | Owned aligned storage, `make_buffer_for!` for concrete generic schemas, and round trips with in-storage borrows. |
| `cross_crate.rs` | Real crate boundaries; public/private child composition without trait leakage; metadata, errors and source identity; const generics, borrows, and tagged nesting. |
| `encode_failure.rs` | Layout/semantic preflight preservation, post-write invalidation confinement, callback/tag precedence, and sentinels around the destination. |
| `error_path_alloc.rs` | Owned direct, nested, generic, and external error paths without duplicated child schema. It also provides a compile-time `alloc`-without-`std` witness. Run its required feature mode with `cargo +1.97.0 test --locked -p zero-schema --test error_path_alloc --no-default-features --features 'alloc,derive'`. |
| `errors.rs` | Error kinds, segments, validation codes and `Display`; downcastable layout/UTF-8/validation sources; wrapper-free nested and tagged paths. |
| `golden.rs` | The six named schema-corpus binaries and SHA-256 values, exact endian/string/union bytes, layout constants, and selected malformed-input errors. |
| `golden_inventory.rs` | LF-only 11-column corpus inventory, complete root/type/fuzz selector registration, safe paths, lengths, and hashes for numeric goldens and fuzz seeds. |
| `invalid_bytes.rs` | Bool, enum, length, UTF-8, NUL, tail, padding and tag corruption; exact offsets/paths; depth-first, external-preread, and validator precedence. |
| `miri.rs` | The focused memory-model suite: borrows/prefixes, alignment and original-byte padding, malformed strings, encode preflight, unions, and nested error sources. It is run by Miri, not as a substitute for the rest of the integration suite. |
| `no_alloc.rs` | The focused single-thread allocator measurement across generated success, failure, formatting, and round-trip paths. It is intentionally ignored in broad parallel suites; see [Ignored tests](#ignored-tests). |
| `prefix.rs` | Short, exact, and extra input; consumption of `WIRE_SIZE` rather than stride; remainder and input immutability across nesting and unions. |
| `properties.rs` | Deterministic 256-case, fixed-seed proptests for logical round trips and borrows, float bits, arbitrary exact bytes without panic, tail/padding behavior, transactional encode, and tag precedence. Run explicitly with `cargo +1.97.0 test --locked -p zero-schema --test properties`. |
| `roundtrip.rs` | Every primitive, float, bool and enum; all four borrowed string forms and fixed bytes; pointers into the source buffer; nested/generic/zero-capacity/internal/external union shapes. |
| `scalar_cross_crate.rs` | Derive-only bounds, metadata and raw-name normalization, plus cross-crate structured-error facts. |
| `scalar_enum.rs` | Non-`Copy` derived API, all supported representations/endian boundaries, prefix/errors, transactional layout checks, and metadata order. |
| `scalar_paths.rs` | Runtime-path rebasing, shadow and generated-name collision resistance, reexports, and raw identifiers. |
| `struct_direct.rs` | Direct fields, prefix/roundtrip behavior, transactional failures, bool precedence, metadata, and padding order. |
| `struct_generic.rs` | Const generics, lifetime-only storage/borrows, and nested generic diagnostics without incidental `Debug`/`Clone`/`Copy` bounds. |
| `struct_hygiene.rs` | Generated declaration-scope paths in the presence of colliding field names. |
| `struct_layout.rs` | Descriptor-versus-wire layout, aligned fields, ordered helper/wrapper padding, and the first parent-owned padding error. |
| `struct_nested.rs` | Nested round trips and metadata, delegated structured errors, and transactional nested encode. |
| `struct_smoke.rs` | End-to-end owned aligned bytes, prefixes and borrows; aligned strings, validators, errors, and semantic destination preservation. |
| `struct_validation.rs` | Declaration/whole callback order and contexts; built-in range/`must_equal` precedence; padding, prefix, borrowed-field and later-field ordering. |
| `tagged_external.rs` | Sibling tags before/after payloads, one cached tag shared by two payloads, known-unmapped tags, mismatch preservation, and all-unit ZST payloads. |
| `tagged_generic.rs` | Type/lifetime-generic tagged storage, projected errors, erased buffers, and nested lifetime errors. |
| `tagged_internal.rs` | Unit/newtype/all-unit round trips, unknown and known-unmapped tags, inactive-tail errors, alignment, and transactional layout behavior. |
| `wire_codec.rs` | Primitive bytes, bool/scalar/helper metadata, all string boundaries/tails/capacities, layout overflow/precedence, and direct zero-allocation codec behavior. |

`tests/support/counting_alloc.rs` is support code, not an independent target. It installs a thread-local counter over `System` and is included by `allocation.rs`, `no_alloc.rs`, and `wire_codec.rs`; those targets are what execute it.

## Derive tests and compile fixtures

Run frontend/generator unit tests with:

```sh
cargo +1.85.0 test --locked -p zero-schema-derive --lib
```

`zero-schema-derive/src/parse.rs` contains the frontend/IR unit suite: option grammar and applicability, literal/capacity/alignment bounds, item shapes and `repr`, tag graphs, range expressions, lifetimes/generics, moved syntax and path rebasing, names/visibility, zero layouts, wide-endian checks, obligations/errors, and symbolic layout order. `zero-schema-derive/src/gen.rs` tests dependency aliases that are absolute, hyphenated, or Rust keywords.

Run compile diagnostics and isolated packages with:

```sh
cargo +1.85.0 test --locked -p zero-schema-derive --test ui
```

`zero-schema-derive/tests/ui.rs` drives two kinds of inputs:

- Trybuild compiles `tests/ui/pass/00_scalar_only.rs` and the registered `tests/ui/fail/01_*` through `14_*` cases with their `.stderr` snapshots. Together they cover valid scalar generation and invalid/poisoned attributes, nested attributes, item shapes/representations, unsupported types and borrows, bounds, empty/tag syntax and linking, expression grammar, generated-name collisions, recursion/macros, zero records, and the missing-zerocopy diagnostic.
- Isolated Cargo packages ensure diagnostics and dependency resolution are not accidentally satisfied by the derive crate's own dev-dependency graph:
  - `aggregate-pass`: a generic zero-length child inside a nonzero parent compiles and encodes.
  - `struct-hygiene-pass`: `deny(warnings)`, renamed dependencies, generics, nested/rebased paths, shadowing, raw names, and visibility compile together.
  - `local-item-fail`: function-local derives demonstrate the documented unsupported resolution case and match bounded diagnostic signatures.
  - `wide-target-fail`: the host-opposite explicit wide endian fails at its field.
  - `missing-zerocopy-fail`: absence of the required direct `zerocopy` dependency produces the targeted diagnostic.
  - `lazy-zero-pass`: the nonzero `N = 1` root and nested parse/encode neighbors build.
  - `lazy-zero-fail`: four bins instantiate zero-sized generic roots/children through root/parent parse/encode and must fail const evaluation.

The previously unregistered UI pass files `01`–`05`, unregistered `fail/15`, and empty `private-reexport-fail` package have been removed. They are therefore neither fixtures nor claimed coverage. The local-item limitation remains executed through `local-item-fail`.

## Shared schema and cross-crate fixtures

These workspace crates contain no standalone `#[test]`; root, conformance, fuzz, and Miri targets compile or call them:

- `test-fixtures/schema-corpus/src/lib.rs` defines the `no_std` fuzz schema graph, root IDs 1–6, and fuzz selector registry. `inventory.csv` and `golden/*.bin` are verified by `golden.rs`, `golden_inventory.rs`, and fuzz unit tests.
- `test-fixtures/schema-corpus/src/conformance.rs` is the 11-root Rust schema graph parsed by `conformance/build.rs`; it is exercised by the conformance package command below.
- `test-fixtures/cross-crate-child/src/lib.rs` supplies foreign scalar, nested, generic, borrowed and tagged schemas plus a trailing-projection witness.
- `test-fixtures/cross-crate-consumer/src/lib.rs` supplies downstream API/privacy/error/metadata/roundtrip helpers. Foreign lifetime-erased helpers are intentionally excluded under `cfg(miri)`; ordinary cross-crate tests exercise them, while Miri stays within its supported focused graph.

Run their behavioral consumers with the workspace gate or the focused `cross_crate`, `golden`, `golden_inventory`, conformance, fuzz, and Miri commands described here.

## `no_std`, link, and cross-endian gates

The following are compile/link proofs rather than host `#[test]` suites:

- `no-std-smoke/src/lib.rs` performs `no_std` round trips for every borrowed string form, fixed bytes, a tagged packet, and prefix parsing. The thumb gate proves the library and generated code compile without `std`:

  ```sh
  cargo +1.85.0 check --locked -p zero-schema-no-std-smoke --lib \
    --target thumbv7em-none-eabihf
  ```

- `no-std-smoke/src/bin/linked-wasm.rs` becomes `no_std`/`no_main` on `wasm32v1-none`, provides a panic handler and `_start`, and calls both smoke functions. The host branch has only an empty `main`; the meaningful proof is the allocator-free target link:

  ```sh
  cargo +1.85.0 build --locked -p zero-schema-no-std-smoke \
    --bin linked-wasm --target wasm32v1-none --release
  ```

- `target-tests/wide-endian/src/lib.rs` is a standalone resolver-3 package with 12 feature-gated `U16Str`/`U16CStr` cases: explicit, inherited, and native-over little/big configurations. `ci/check-wide-endian.sh` selects one feature at a time, requires explicit/inherited endian to match the target, and requires native forms on both profiles:

  ```sh
  ci/check-wide-endian.sh little
  ci/check-wide-endian.sh big powerpc64-unknown-linux-gnu
  ```

The big-endian command requires the configured cross compiler, linker, Rust target, and QEMU runner used by CI.

## C++ conformance and goldens

`conformance/build.rs` parses `test-fixtures/schema-corpus/src/conformance.rs` and `conformance/fixtures/cases.rs`. `build/frontend.rs` validates the frozen input model; `build/layout_cpp.rs` generates standard-layout, ZST-aware C++; and `build/codec_cpp.rs` generates the three staged C ABI functions. `conformance/src/inventory.rs` is the independent Rust oracle, `ffi.rs` is the safety-reviewed adapter and fault injector, and `report.rs` decodes framed reports.

`conformance/fixtures/cases.rs` defines exactly 11 cases (IDs 1001–1011) spanning scalar endian/layout, alignment, empty and data unions, all primitive/float bit patterns, scalar enums, every string form and fixed bytes, nesting, ABI ZST alignment, and external data/unit messages. It is generated-code input, not a Rust test target.

The active test files are:

| File | Contract protected |
| --- | --- |
| `conformance/src/tests/contract.rs` | Agreement of IDs/keys across sources, both codec directions for every case, accepted unaligned inspection input, and the external all-unit parent/child layout witness. |
| `conformance/src/tests/status.rs` | Stable status bytes and unknown values; null/misalignment/ID/length/capacity precedence; written-count reset; input/output/sentinel preservation; exact and excess writes. |
| `conformance/src/tests/report.rs` | Valid framed reports and structured rejection of duplicate/wrong keys, lengths, and pair counts. |
| `conformance/src/tests/golden.rs` | Strict manifest grammar, exact file set and hashes, current Rust/C++ profile equality, and reviewed cross-profile/endian invariants. It also owns the intentionally ignored updater. |

`conformance/fixtures/golden/manifest.csv` and its five profile directories contain 55 reviewed binaries: 11 each for Linux x86_64 little-endian, Linux i686 little-endian, macOS arm64 little-endian, Windows MSVC x86_64 little-endian, and Linux powerpc64 big-endian. They are assertions, not generated build output.

```sh
cargo +1.97.0 test --locked -p zero-schema-conformance
CXX=g++ cargo +stable test --locked -p zero-schema-conformance \
  --target-dir target/compat-gcc
CXX=clang++ cargo +stable test --locked -p zero-schema-conformance \
  --target-dir target/compat-clang
```

CI additionally runs pinned GCC, Clang, ASan+UBSan, i686, AppleClang, MSVC, and powerpc64/QEMU configurations. Conformance tests are disabled under `cfg(miri)` because they call C++ FFI; native and sanitizer jobs own that boundary.

## Fuzzing and corpus

`fuzz/` is a standalone, workspace-excluded package with its own lockfile.

- `fuzz/src/lib.rs` implements bounded selector/payload dispatch, stable double parsing, and round-trip reparse. Its three unit tests validate inventory hashes and seed semantics, malformed external/string inputs, and empty/short/exact/oversized input handling.
- `fuzz/fuzz_targets/parse_message.rs`, `parse_external_tag.rs`, `parse_all_strings.rs`, and `roundtrip_message.rs` are the four `no_main` libFuzzer adapters. Their manifest entries set `test = false`, so campaigns—not `cargo test`—execute them.
- `fuzz/corpus/` contains 110 reviewed seeds (23 message, 17 external, 53 strings, 17 roundtrip), including registered valid/invalid selectors and retained opaque discoveries. Corpus immutability and hashes are part of the test contract.
- `fuzz/artifacts/` and `target/fuzz-artifacts/` are generated failure evidence, not coverage fixtures.

```sh
cargo +nightly-2025-11-15 metadata --locked \
  --manifest-path fuzz/Cargo.toml --format-version 1
cargo +nightly-2025-11-15 test --locked \
  --manifest-path fuzz/Cargo.toml --lib
ci/run-fuzz.sh parse_message
ci/run-fuzz.sh parse_external_tag
ci/run-fuzz.sh parse_all_strings
ci/run-fuzz.sh roundtrip_message
ci/check-fuzz-clean.sh
```

`ci/run-fuzz.sh` accepts one target argument, fuzzes against a temporary writable corpus for exactly 10,000 runs, and proves the reviewed corpus and lockfile remain unchanged. `ci/test-run-fuzz.sh` uses a fake Cargo executable to test unchanged success, added/rewritten discovery rejection and archival, and lockfile mutation rejection:

```sh
ci/test-run-fuzz.sh
```

## Miri

The single supported Miri entry point is intentionally narrow:

```sh
cargo +nightly-2025-11-15 miri test --locked \
  -p zero-schema --test miri --all-features
```

It executes `tests/miri.rs`. It does not run proptest, trybuild, the counting allocator, or C++ FFI. Those layers have separate deterministic/native gates rather than being silently skipped under a broad Miri invocation.

## Benchmarks

Benchmarks first verify the compared implementations and then report measurements; they are not pass/fail performance thresholds.

- `benches/codec.rs`: Criterion compares generated and handwritten encode/decode for a warm single slot and a 64 MiB coprime ring, reporting throughput after correctness checks.
- `conformance/benches/cpp_codec.rs`: Criterion benchmarks C++ case 1010 write/inspect after a correctness warmup, in batches of 1,024, subtracting an empty-call baseline and reporting active-byte throughput.

```sh
cargo +1.97.0 bench --locked -p zero-schema --bench codec
cargo +1.97.0 bench --locked -p zero-schema-conformance --bench cpp_codec
```

Both targets declare `harness = false`; ordinary `cargo test` does not execute them.

## Shell automation tests

This executable harness tests orchestration logic without running a real fuzz campaign:

```sh
ci/test-run-fuzz.sh
```

`ci/test-run-fuzz.sh` covers fuzz corpus/lock immutability and failure archival as described above. Other `ci/*.sh` files are production gates invoked by workflows; they are not presented as independently tested fixtures unless this harness exercises them.

Package-level integration is also checked by `ci/verify-package-pair.sh`, which extracts package archives, creates an isolated graph, and runs Rust 1.85 consumers including derive. This is package verification, not an ordinary unit test.

## Workflow coverage

- `.github/workflows/ci.yml` covers Rust 1.85/stable/1.97 workspace builds and tests, derive UI, docs, focused feature tests, `no_std` targets, native macOS/Linux/Windows conformance, pinned GCC/Clang/sanitizers/QEMU, cross-endian checks, shell harnesses, package archive verification, and workflow-policy checks.
- `.github/workflows/fuzz.yml` is the pull-request fuzz gate: it runs the pinned-nightly fuzz library tests and all four 10,000-run campaign targets, then checks cleanup/evidence and a terminal gate.
- `.github/workflows/miri.yml` runs only the supported focused Miri command above.
- `.github/workflows/verification.yml` runs on every push and pull request, calls CI and Miri, adds fuzz verification for pull requests, and enforces a terminal result gate.

Reusable workflow terminal gates intentionally treat failed, cancelled, or skipped required jobs as failures. A workflow call is coverage only when its terminal gate is green; an earlier job from another run is not substituted.

## Ignored tests

Exactly two tests are intentionally ignored:

1. `tests/no_alloc.rs` must run alone because a process-global allocator measurement would be invalid under the broad parallel suite:

   ```sh
   cargo +1.97.0 test --locked -p zero-schema --test no_alloc \
     -- --ignored --test-threads=1
   ```

2. `conformance/src/tests/golden.rs::regenerate_current_profile` mutates reviewed golden fixtures and is never routine verification. Review the generated byte changes and invoke it only with explicit acceptance:

   ```sh
   ZERO_SCHEMA_ACCEPT_GOLDENS=macos-aarch64-le \
     cargo +1.97.0 test --locked -p zero-schema-conformance \
       tests::golden::regenerate_current_profile -- --ignored --exact --test-threads=1
   ```

   Replace `macos-aarch64-le` with the exact current build profile when running on
   another reviewed target; the guard rejects a profile that does not match the build.

The normal conformance tests verify the committed manifest and binaries without modifying them.
