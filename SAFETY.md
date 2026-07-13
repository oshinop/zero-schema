# Safety

## Scope

`zero-schema` treats input as untrusted bytes and exposes borrowed views only after
proving the required size and alignment. The publishable runtime,
`zero-schema-derive`, and generated expansions contain no handwritten `unsafe`
blocks or unsafe implementations. Safety relies on safe Rust plus `zerocopy`
`FromBytes`, `KnownLayout`, and `Immutable` contracts.

This document covers memory safety. It does not promise that a user validator is
pure, bounded, allocation-free, or panic-free, nor that an application publishes a
semantically valid message before checking an encoding result.

## Decode argument

1. Every wire type is all-bit-valid and contains no references. Aggregate and union
   wires derive only `FromBytes`, `KnownLayout`, and `Immutable`; they do not derive
   `IntoBytes`.
2. Exact and prefix decoding check length before alignment. A typed view is created
   only for the accepted byte range and only when its address satisfies the wire's
   alignment.
3. A decode input retains both the typed view and the exact original byte slice.
   Checked subranges prove offset addition, bounds, and the child alignment before
   creating another view. Padding is inspected in the original bytes rather than by
   converting an aggregate wire to bytes.
4. Borrowed `str` is produced only after a checked length and UTF-8 validation.
   `CStr` requires an in-capacity NUL. Wide string views use native `u16` storage,
   checked capacity/termination rules, and remain tied to the input lifetime.
5. Scalar enums decode integer wire values and reject unknown discriminants. Tagged
   unions select a payload only after decoding a known tag; no Rust union field is
   read through an unsafe access.
6. Generated outlives bounds allow a source borrow to be shortened but never
   extended. The lifetime-free wire type cannot contain a second byte source.

Consequently, successful decoding yields only owned scalar values or immutable
borrows within the original live input slice.

## Encode argument

1. Public encoding checks exact destination size and alignment, then performs
   semantic validation.
2. The root destination is filled with zero exactly once. Nested encoders receive
   checked, confined subranges and cannot obtain the mutable backing slice.
3. Writes use checked offset addition and bounds. Primitive codecs write explicit
   byte arrays. String and fixed-array codecs copy only validated logical content;
   unused bytes remain initialized zero.
4. Encoding never obtains bytes from aggregate or union wire values. The only
   borrowed native-unit byte view is zerocopy's safe `IntoBytes` implementation for
   `[u16]`.
5. `AlignedBytes<W, N>` stores initialized `[u8; N]` and uses a zero-length wire
   array solely to impose alignment; it does not expose `MaybeUninit`. Its byte view
   excludes trailing stride padding. `make_buffer_for!(FullyConcreteType)` constructs it
   without exposing the wire projection.

For monomorphic and lifetime-only schemas, `encode()` owns its output and its return
type is independent of borrowed input lifetimes. On error that private output is
dropped. `encode_into` exists for caller-owned storage and fully concrete generic
schemas. A semantic preflight failure currently leaves that destination unchanged;
the public contract intentionally permits a later error to invalidate destination
contents, so callers must publish only after `Ok(())`.

## Layout and bounded work

Generated constants assert wire size, alignment, stride, field offsets, and padding
ranges. Slot multiplication and all offset/end calculations are checked. Work is
bounded by the fixed wire size and declared capacities, except for work performed by
user validators and caller-selected formatting/allocation. Generated codec and
structured-error traversal paths allocate nothing; the optional owned error path
uses `alloc` normally.

Validators receive immutable projected values and immutable context only. They do
not receive source bytes or mutable destination access. Validator authors must make
callbacks total and nonpanicking for every projected value to preserve the
arbitrary-byte no-panic property.

## Unsafe inventory

Published artifacts:

- `zero-schema`: no handwritten unsafe code.
- `zero-schema-derive`: no handwritten unsafe code.
- generated schema code: no handwritten unsafe code.

Unpublished repository support contains the complete intentional inventory:

- `tests/support/counting_alloc.rs`: an `unsafe impl GlobalAlloc` and calls to the
  system allocator. Each operation forwards the allocator contract unchanged and
  records only successful returned pointers.
- `conformance/src/ffi.rs`: C ABI declarations and calls plus deliberately malformed
  pointer construction for fault-precedence tests. Valid calls prove pointer
  validity, alignment, initialized input, capacity, and writable output; malformed
  pointers are test inputs to C++ functions whose protocol checks them before any
  dereference.
- `no-std-smoke/src/bin/linked-wasm.rs`: `#[unsafe(no_mangle)]` exports `_start`; its
  body uses safe Rust and calls smoke functions.

The C++ conformance fixture uses byte storage and `memcpy`, never type punning, and
is exercised separately from Miri. The focused Miri suite covers Rust decoding,
borrows, alignment failures, padding scans, union selection, errors, buffers, and
round trips. Native sanitizer jobs cover the C++ boundary.

## Interoperability boundary

Memory safety does not make two independently compiled layouts compatible. A
conforming C++ mirror must match the target ABI/data model/endian profile, use
C++17 exact-width types and default non-packed layout, and satisfy the generated
size/alignment/offset assertions. Packing, bitfields, C++ `bool` or enum storage,
`wchar_t`, ABI-changing flags, and unmatched native-wide endianness are outside the
contract. The conformance build generates unpublished test mirrors only; there is no
consumer header generator or fingerprint-based negotiation.

Security or safety concerns should be reported privately to the project maintainers
through the hosting platform's security-reporting channel when one is configured.
