# Testing zero-schema

This is the maintainer map for the `0.1.0` producer-byte capability contract. Test
layers are complementary: runtime units cover bounded primitives, integration targets
exercise generated public capabilities over reviewed producer bytes, UI fixtures pin
attribute diagnostics, and target/FFI/fuzz/Miri/package gates cover properties a host
unit test cannot establish.

Run commands from the repository root. CI uses `--locked`; the commands below do too.

## Toolchains and fast entry points

| Toolchain | Role |
| --- | --- |
| Rust `1.85.0` | MSRV and primary behavioral gate. |
| moving `stable` | Compatibility, lint, documentation, and native conformance gate. |
| Rust `1.97.0` | Pinned formatting, lint, focused feature, benchmark, and package tooling gate. |
| `nightly-2026-07-02` | Miri and fuzz pin; it is `rustc 1.98.0-nightly (4c9d2bfe4 2026-07-01)`. |

```console
# Runtime units and the root integration suite.
cargo +1.85.0 test --locked -p zero-schema --lib
cargo +1.85.0 test --locked -p zero-schema --tests

# MSRV workspace behavioral gate; macro UI is a separate package gate.
cargo +1.85.0 test --locked --workspace --all-features --exclude zero-schema-macros --lib --tests
cargo +1.85.0 test --locked -p zero-schema-macros --lib
cargo +1.85.0 test --locked -p zero-schema-macros --test ui
cargo +1.85.0 test --locked -p zero-schema-macros --test mutation_api

# Primary end-to-end producer/capability proof.
cargo +1.85.0 test --locked -p zero-schema --test focused_capabilities
```

The root package's default feature set is `std`; `alloc` is its only other feature.
`zero-schema` unconditionally depends on and re-exports `#[zero]` from the
`zero-schema-macros` attribute package. Macro availability enables neither `alloc` nor
`std` and does not change target-binary behavior. Do not substitute an all-default
`cargo test` result for the feature, target, UI, FFI, Miri, or package layers below.

## Runtime unit modules

```console
cargo +1.85.0 test --locked -p zero-schema --lib
```

This one target runs the inline units in these source modules:

| Module | Contract protected |
| --- | --- |
| `src/access.rs` | Exact-size-first/alignment checks, checked bounded selections, and receiving storage. |
| `src/array.rs` | O(1) views, indexed operations, and array preflight-before-commit. |
| `src/error.rs` | Allocation-free error kind/path/source traversal and formatting. |
| `src/layout.rs` | Diagnostic descriptor and ABI metadata construction. |
| `src/lib.rs` | Public receiving-storage macro and exports. |
| `src/mutation.rs` | Scalar/string/fixed-byte preflight and confined writes. |
| `src/strings.rs` | Narrow/wide active-length, UTF-8, terminator, and bounded-write rules. |
| `src/tagged.rs` | External-tag selection and payload-before-tag commit mechanics. |
| `src/wire.rs` | All-bit-valid scalar storage, endian loads/stores, and Boolean representation. |

## Root integration targets

Cargo metadata registers the following 23 root integration targets. Run one focused target
with:

```console
cargo +1.85.0 test --locked -p zero-schema --test <target>
```

| Target | Contract protected |
| --- | --- |
| `allocation` | Access, reads, arrays, selected unions, patches, and error formatting allocate zero times. |
| `capability_access` | Reviewed bytes yield borrowed field capabilities, diagnostic metadata, and short mutable reborrows. |
| `cross_crate` | Public/private composition, generic descriptors, errors, and tagged payloads across real crate boundaries. |
| `error_paths` | Eager nested access paths retain field, index, and variant structure. |
| `exact_span` | Callers choose exact stride-sized slots; no remainder-taking root API exists. |
| `focused_capabilities` | Full representative schema: exact/aligned proof, reads, mutation, arrays, patches, external unions, ignored bytes, and atomic errors. |
| `golden` | Reviewed scalar, record, string, and external-union producer fixtures plus frozen layout/error facts. |
| `golden_inventory` | Corpus inventory grammar, registration, paths, lengths, and hashes. |
| `generic_lifetime_regression` | Source-lifetime rebinding and generic nested-schema composition across a downstream crate. |
| `invalid_bytes` | Deterministic eager rejection of invalid Boolean, enum, strings, selected payloads, and arrays while ignored storage stays irrelevant. |
| `macro_hygiene` | Runtime-path override, raw identifiers, and shadowed-prelude expansion over producer bytes. |
| `miri` | Focused provenance, borrow, mutation, patch, and ignored-storage suite; execute it through Miri below. |
| `mutation_atomicity` | Failed field, array, tag-only, and mismatched-union preflight preserves every byte. |
| `no_alloc` | Single-thread allocation instrumentation for core capability operations. |
| `optional_zero_sentinel` | Zero-sentinel `Option` storage and metadata for zero-invalid enum/schema/array paths, complete-span clearing, atomic `OptionMut` and tri-state patches, and eager malformed-present diagnostics. |
| `producer_fixture` | Independent reviewed fixture identity, alignment, hash, and nonzero ignored storage. |
| `private_capability_safety` | Safe-code compile failures pin private proof inputs, immutable capabilities, and absence of raw aggregate wire access. |
| `properties` | Fixed-seed arbitrary initialized bytes never panic; producer access/no-op patch stays valid. |
| `receiving_storage` | `schema_buffer!` named types and `make_schema_buffer!` values preserve exact size/alignment and receiving-only semantics, including concrete generic roots. |
| `roundtrip` | Producer access → logical materialization → patch → fresh access transfer contract. |
| `scalar_cross_crate` | Scalar capabilities, metadata, and structured errors across a downstream crate. |
| `scalar_enum` | Scalar-root exact access, closed discriminants, mutation, patch, and metadata. |
| `tagged_external` | Independent external tags, selected payload capabilities, complete switches, and tag/payload patch rules. |

`tests/support/producer.rs` copies reviewed binary fixtures into explicitly aligned
storage. It is fixture support, not a schema producer. `tests/support/counting_alloc.rs`
is used by `allocation` and `no_alloc`; it is not an independent Cargo target.

## Attribute macro units, UI, and isolated fixtures

```console
cargo +1.85.0 test --locked -p zero-schema-macros --lib
cargo +1.85.0 test --locked -p zero-schema-macros --test ui
cargo +1.85.0 test --locked -p zero-schema-macros --test mutation_api
```

The macro library target covers syntax analysis and generated support. The `ui` harness
explicitly registers 12 passing sources (`00_retained_item` through
`11_symbolic_nonzero_arrays`) and 76 compile-fail sources, each with a reviewed
diagnostic fixture. The failures cover removed surface requests, exact option grammar,
invalid item/tag relationships, tagged payload root/storage attempts, zero-size and
array restrictions, recursion, wide-string option scope, generated-name collisions,
and zero-sentinel `Option` eligibility: only zero-invalid enum/schema paths and fixed
arrays are accepted; direct primitives, Boolean, strings/bytes, tagged payloads, nested
`Option`, element-level optional arrays, zero-valid records/scalars/tagged variants,
and zero-length or nested symbolic zero-length arrays are rejected.

The privacy and capability-closure fixtures include `28_hidden_wire_and_legacy_api`,
`31_hidden_proof_bypass` through `40_hidden_wire_copy`,
`50_private_option_initialization`, and `51_optional_wire_and_legacy_surface` through
`55_option_mut_literal`. The final surface is `copy_into`, transactional `copy_from`,
`ArrayRef`, `ArrayRefIter`, and `TaggedRefSelection`; generated `<Logical>Wire` types
remain opaque. The isolated `legacy-naming-surface-fail` package rejects the removed
materialization, view, physical-wire, and tagged-selection symbols.

The same UI test also builds isolated packages:

- passing: `aggregate-pass`, `criterion-import-pass`, and
  `renamed-dependencies-pass`;
- expected failures: `local-item-fail`, `missing-zerocopy-fail`,
  `missing-tag-field-fail`, `wrong-tag-enum-fail`, and
  `legacy-naming-surface-fail`; and
- a cross-crate consumer library check.

`mutation_api` exercises generated field and array mutation, `copy_into`,
transactional `copy_from`, tagged payload changes, zero-sentinel option mutation and
tri-state patches, borrowed storage, and failure atomicity.

These packages ensure the macro's dependency resolution and visibility behavior do
not accidentally rely on the macro crate's own development graph.

## Features, docs, and focused maintenance checks

MSRV feature compilation is explicit:

```console
cargo +1.85.0 check --locked --workspace --all-targets
cargo +1.85.0 check --locked -p zero-schema --no-default-features
cargo +1.85.0 check --locked -p zero-schema --no-default-features --features alloc
cargo +1.85.0 check --locked -p zero-schema --no-default-features --features std
ci/check-macros-dev-features.sh 1.85.0
cargo +1.85.0 test --locked --doc --workspace
```
`#[zero]` is available in every root feature selection; the unconditional procedural-
macro dependency does not enable `alloc` or `std`. The `optional` example has no
feature gate. The existing `--no-default-features` checks intentionally select only the
root library because several integration targets require `alloc`; they nevertheless
keep declarations available without `alloc` or `std`. In CI, the MSRV workspace
`--all-targets` command compiles the default feature set (including `optional`), and
its all-feature root integration command runs `optional_zero_sentinel`; the no-std job
separately builds `no_std_wasm` for `wasm32v1-none`; and the separate MSRV macro
commands run both `ui` and `mutation_api`.

The pinned focused feature/allocation/property and documentation checks are:

```console
cargo +1.97.0 test --locked -p zero-schema --test error_paths --no-default-features --features alloc
cargo +1.97.0 test --locked -p zero-schema --test no_alloc -- --test-threads=1
cargo +1.97.0 test --locked -p zero-schema --test properties
cargo +1.97.0 test --locked --doc --workspace
RUSTDOCFLAGS='-D warnings' cargo +1.97.0 doc --locked --workspace --all-features --no-deps
```

Moving `stable` owns the broad lint and documentation checks:

```console
cargo +stable clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo +stable check --locked --workspace --benches --all-features
cargo +stable test --locked --workspace --all-features --exclude zero-schema-macros --lib --tests
cargo +stable test --locked -p zero-schema-macros --lib
cargo +stable test --locked -p zero-schema-macros --test mutation_api
cargo +stable test --locked --doc --workspace
RUSTDOCFLAGS='-D warnings' cargo +stable doc --locked --workspace --all-features --no-deps
```

Pinned formatting checks cover the root, fuzz, and wide-endian manifests. Pinned
Clippy checks cover the root workspace and fuzz manifest; see `.github/workflows/ci.yml`.

## Core-only and endian target proofs

The unpublished `zero-schema-no-std-smoke` package depends on `zero-schema` with
default features disabled and direct `zerocopy`. It declares `#[zero]` schemas without
enabling `alloc` or `std`; its target commands are:

```console
rustup target add --toolchain 1.85.0 thumbv7em-none-eabihf wasm32v1-none
cargo +1.85.0 check --locked -p zero-schema-no-std-smoke --lib --target thumbv7em-none-eabihf
cargo +1.85.0 build --locked -p zero-schema-no-std-smoke --bin linked-wasm --target wasm32v1-none --release
```

The first is a core-only compile proof; the second is a freestanding wasm link proof.
Neither runs target code.

`target-tests/wide-endian` keeps a deliberately narrow matrix. The aggregate
`positive-fixtures` feature is valid on either endian profile; scalar/length-prefix
endianness is tested independently while `U16Str`/`U16CStr` units remain native.
Only explicit `endian` options on `U16CStr` are expected failures.

```console
# Native little-endian positive fixture test plus the three exact U16CStr diagnostics.
ci/check-wide-endian.sh little

# CI-only big-endian equivalent; requires the configured target, linker, and QEMU.
ci/check-wide-endian.sh big powerpc64-unknown-linux-gnu
```

## C++ conformance and reviewed fixtures

`zero-schema-conformance` compares Rust capabilities with C++17 producer/observer
fixtures. Its ordinary native commands are:

```console
cargo +stable test --locked -p zero-schema-conformance
CXX=g++ cargo +stable test --locked -p zero-schema-conformance --target-dir target/compat-gcc
CXX=clang++ cargo +stable test --locked -p zero-schema-conformance --target-dir target/compat-clang
```

The conformance test modules divide responsibility explicitly:

| Module | Contract protected |
| --- | --- |
| `contract` | Frozen and native case identities/layout keys; C++ production, Rust capability reads, and C++ observation agree. |
| `golden` | Frozen manifest grammar, profile/case inventory, lengths, hashes, and guarded fixture regeneration. |
| `options` | Native C++ zero-sentinel `Option` cases, complete-span clearing, padding, malformed-present rejection, and Rust/C++ observation agreement. |
| `report` | Strict report length, key order, uniqueness, and pair-count parsing. |
| `status` | Stable status mapping, pointer/alignment/length precedence, and failure sentinel preservation. |

The native C++ registry has 15 IDs: the frozen ten
`1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1010, 1011`, plus native-only
zero-sentinel `Option` cases `1012, 1013, 1014, 1015, 1016` (None; enum, child,
array, and all-Some). ID `1009` is absent. The native-only cases are intentionally not
added to the reviewed foreign-profile golden manifest: those profiles cannot be
regenerated truthfully. The frozen inventory therefore remains exactly the ten IDs
above across five reviewed profile directories and 50 golden rows/binaries:

| Rust target | Profile |
| --- | --- |
| `aarch64-apple-darwin` | `macos-aarch64-le` |
| `x86_64-unknown-linux-gnu` | `linux-x86_64-le` |
| `i686-unknown-linux-gnu` | `linux-i686-le` |
| `x86_64-pc-windows-msvc` | `windows-x86_64-msvc-le` |
| `powerpc64-unknown-linux-gnu` | `linux-powerpc64-be` |

Any other target is intentionally unsupported by this conformance profile selector.
CI separately runs GCC, Clang, ASan/UBSan, i686/QEMU, and powerpc64/QEMU. C++ FFI is
not exercised under Miri.

The sole routine-excluded conformance action rewrites reviewed bytes and must be
requested explicitly with the current profile:

```console
ZERO_SCHEMA_ACCEPT_GOLDENS=<profile> \
  cargo +1.97.0 test --locked -p zero-schema-conformance \
  tests::golden::regenerate_current_profile -- --ignored --exact --test-threads=1
```

Review the changed fixture bytes and manifest before accepting them.

## Fuzzing and Miri

`fuzz/` is workspace-excluded and retains its own lockfile. The nightly pin and its
four campaigns are exact:

```console
rustup toolchain install nightly-2026-07-02 --profile minimal
cargo +nightly-2026-07-02 install --locked --version 0.13.2 cargo-fuzz
cargo +nightly-2026-07-02 metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1
cargo +nightly-2026-07-02 test --locked --manifest-path fuzz/Cargo.toml --lib
ci/run-fuzz.sh parse_message
ci/run-fuzz.sh parse_external_tag
ci/run-fuzz.sh parse_all_strings
ci/run-fuzz.sh roundtrip_message
ci/check-fuzz-clean.sh
```

The fuzz library unit target checks its reviewed seed inventory and dispatch contract.
`TARGET_COUNTS` assigns three schema cases to `parse_message` and one to each other
campaign; each campaign exercises eager access, logical `copy_from`, and bounded
mutation/round-trip paths without treating scratch bytes as an initialized schema.

Each CI campaign uses a temporary writable corpus for exactly 10,000 runs, records
its evidence, and proves the reviewed corpus and lockfile remain unchanged. The shell
runner itself is covered by:

```console
ci/test-run-fuzz.sh
```

Miri runs only the dedicated focused harness; it does not replace native conformance,
trybuild, property, or allocation layers:

```console
rustup toolchain install nightly-2026-07-02 --profile minimal --component miri
cargo +nightly-2026-07-02 miri test --locked --manifest-path tests/miri-harness/Cargo.toml --test miri
```

## Benchmarks, package archives, and workflows

Benchmarks compile and execute capability-oriented comparisons plus zero-sentinel
`Option` access, mutation, tri-state patch promotion, and materialization. They are
smoke coverage, not pass/fail performance thresholds:

```console
cargo +1.97.0 bench --locked -p zero-schema --bench codec
cargo +1.97.0 bench --locked -p zero-schema-conformance --bench cpp_codec
```

The root manifest registers seven executable contract examples. Only `access_errors`
requires `alloc`; `#[zero]` is available in every configuration, so no example is
gated on declaration availability:

| Example | Contract demonstrated |
| --- | --- |
| `records` | Nested reads, `ArrayRef`/`ArrayMut`, metadata, partial patches, ignored padding, and byte-exact failed mutation. |
| `strings` | All four bounded borrowed string forms, fixed bytes, endian conversion, constrained setters, and capacity failure atomicity. |
| `tagged` | External-tag selection, selected materialization/mutation, rejected tag-only patches, and payload-before-tag switching. |
| `optional` | Eligible scalar/schema/array zero sentinels, `OptionMut`, complete promotion, and tri-state patches. |
| `access_errors` | Scalar-enum roots plus structured length, alignment, Boolean, and enum access failures. |
| `generic_receiving_buffer` | Concrete generic `schema_buffer!` type naming, alignment, metadata, and producer-byte receipt. |
| `no_std_wasm` | Freestanding core-only external-union access on `wasm32v1-none`. |

```console
cargo +1.85.0 run --locked --example records
cargo +1.85.0 run --locked --example strings
cargo +1.85.0 run --locked --example tagged
cargo +1.85.0 run --locked --example access_errors --no-default-features --features alloc
cargo +1.85.0 run --locked --example generic_receiving_buffer
cargo +1.85.0 build --locked --example no_std_wasm --target wasm32v1-none --release --no-default-features
cargo +1.85.0 run --locked --example optional
```

The workspace has exactly seven members: `zero-schema`, `zero-schema-macros`,
`zero-schema-conformance`, `zero-schema-no-std-smoke`, and the schema-corpus,
cross-crate-child, and cross-crate-consumer fixture packages. `fuzz/` and
`target-tests/wide-endian` are workspace-excluded standalone manifests; fuzz owns its
own lockfile. `tests/miri-harness` is also standalone. Package verification archives
only the publishable runtime and macro packages.

Package verification checks both archives as one consumer graph. It inventories first,
then packages the macro crate and runtime together, and finally verifies a consumer
that directly declares `zerocopy`, imports `zero_schema::zero`, accesses producer
bytes, materializes, mutates, and uses both receiving-storage macros:

```console
cargo +1.97.0 install --locked --version 0.20.2 cargo-deny
cargo +1.97.0 deny --locked --workspace --all-features check
cargo +1.97.0 deny --locked --all-features --manifest-path fuzz/Cargo.toml check
cargo +1.97.0 deny --locked --all-features --manifest-path target-tests/wide-endian/Cargo.toml check
mkdir -p target/package
cargo +1.97.0 package --registry crates-io --locked --list -p zero-schema-macros > target/package/zero-schema-macros.files
diff -u ci/package-contents/zero-schema-macros.txt target/package/zero-schema-macros.files
cargo +1.97.0 package --registry crates-io --locked --list -p zero-schema > target/package/zero-schema.files
diff -u ci/package-contents/zero-schema.txt target/package/zero-schema.files
cargo +1.97.0 package --registry crates-io --locked -p zero-schema-macros -p zero-schema
ci/verify-package-pair.sh target/package/zero-schema-macros-0.1.0.crate target/package/zero-schema-0.1.0.crate
```

`.github/workflows/ci.yml` owns MSRV/stable/pinned Rust gates, core-only targets,
native and cross C++ conformance, wide-endian, package, and workflow-policy jobs.
`.github/workflows/miri.yml` owns the one Miri command above;
`.github/workflows/fuzz.yml` owns the four campaign names; and
`.github/workflows/verification.yml` requires the reusable CI, Miri, and fuzz gates
to succeed.
