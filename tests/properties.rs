use core::ffi::CStr;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRunner};
use widestring::{U16CStr, U16Str};
use zero_schema::{ErrorKind, FieldKind, SchemaError, TypeKind, ZeroSchema};

const CASES: u32 = 256;
const SEED: u64 = 0x5a53_3031_5f50_524f;

fn committed_config() -> Config {
    Config {
        cases: CASES,
        failure_persistence: None,
        rng_algorithm: RngAlgorithm::ChaCha,
        rng_seed: RngSeed::Fixed(SEED),
        fork: false,
        timeout: 0,
        ..Config::default()
    }
}

fn runner() -> TestRunner {
    TestRunner::new(committed_config())
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
#[zero(endian = "little")]
enum Mode {
    Idle = 0,
    Active = 0x102,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    signed: i32,
    flag: bool,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
struct Logical<'a> {
    byte: u8,
    word: u16,
    signed: i64,
    flag: bool,
    mode: Mode,
    child: Child,
    #[zero(capacity = 12, len_type = u8, tail = "zero")]
    text: &'a str,
    #[zero(capacity = 9, tail = "zero")]
    ctext: &'a CStr,
    #[zero(capacity = 8, len_type = u8, tail = "zero")]
    wide: &'a U16Str,
    #[zero(capacity = 8, tail = "zero")]
    wide_c: &'a U16CStr,
    fixed: &'a [u8; 5],
}

#[derive(Debug, ZeroSchema)]
struct Floats {
    f32_value: f32,
    f64_value: f64,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Payload {
    value: u32,
}

#[derive(ZeroSchema)]
#[repr(u8)]
enum ChoiceTag {
    Unit = 1,
    Data = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = ChoiceTag, tail = "zero")]
enum Choice {
    #[zero(tag = ChoiceTag::Unit)]
    Unit,
    #[zero(tag = ChoiceTag::Data)]
    Data(Payload),
}

fn ptr_range<T>(pointer: *const T, units: usize, input: &[u8]) -> bool {
    let start = input.as_ptr() as usize;
    let end = start + input.len();
    let value = pointer as usize;
    value >= start
        && value
            .checked_add(units * core::mem::size_of::<T>())
            .is_some_and(|v| v <= end)
}

#[test]
fn committed_runner_ignores_environment_configuration() {
    // Environment variables are intentionally never read: this exact value is passed to TestRunner.
    let config = committed_config();
    assert_eq!(config.cases, 256);
    assert!(config.failure_persistence.is_none());
    assert_eq!(config.rng_algorithm, RngAlgorithm::ChaCha);
    assert_eq!(config.rng_seed, RngSeed::Fixed(SEED));
    assert!(!config.fork);
    assert_eq!(config.timeout, 0);
    for key in [
        "PROPTEST_CASES",
        "PROPTEST_RNG_ALGORITHM",
        "PROPTEST_RNG_SEED",
        "PROPTEST_FORK",
        "PROPTEST_TIMEOUT",
    ] {
        let _ = std::env::var_os(key); // Presence cannot affect the already-committed configuration.
        assert_eq!(committed_config().cases, CASES);
        assert_eq!(committed_config().rng_seed, RngSeed::Fixed(SEED));
    }
}

#[test]
fn logical_release_shapes_roundtrip_and_borrow_input() {
    let strategy = (
        any::<u8>(),
        any::<u16>(),
        any::<i64>(),
        any::<bool>(),
        any::<bool>(),
        any::<i32>(),
        proptest::collection::vec(0x20u8..0x7fu8, 0..=12),
        proptest::collection::vec(1u8..=0xff, 0..=8),
        proptest::collection::vec(any::<u16>(), 0..=8),
        proptest::collection::vec(1u16..=0xffff, 0..=7),
        any::<[u8; 5]>(),
    );
    runner()
        .run(
            &strategy,
            |(
                byte,
                word,
                signed,
                flag,
                active,
                child_signed,
                text_bytes,
                cbytes,
                wide_units,
                wide_c_units,
                fixed,
            )| {
                let text = core::str::from_utf8(&text_bytes).unwrap();
                let mut c_storage = cbytes;
                c_storage.push(0);
                let ctext = CStr::from_bytes_with_nul(&c_storage).unwrap();
                let wide = U16Str::from_slice(&wide_units);
                let mut wide_c_storage = wide_c_units;
                wide_c_storage.push(0);
                let wide_c = U16CStr::from_slice(&wide_c_storage).unwrap();
                let value = Logical {
                    byte,
                    word,
                    signed,
                    flag,
                    mode: if active { Mode::Active } else { Mode::Idle },
                    child: Child {
                        signed: child_signed,
                        flag: !flag,
                    },
                    text,
                    ctext,
                    wide,
                    wide_c,
                    fixed: &fixed,
                };
                let mut buffer = zero_schema::make_buffer_for!(Logical<'static>);
                value.encode_into(buffer.as_bytes_mut()).unwrap();
                let decoded = Logical::parse(buffer.as_bytes()).unwrap();
                prop_assert_eq!(&decoded, &value);
                prop_assert!(ptr_range(
                    decoded.text.as_ptr(),
                    decoded.text.len(),
                    buffer.as_bytes()
                ));
                prop_assert!(ptr_range(
                    decoded.ctext.as_ptr(),
                    decoded.ctext.to_bytes_with_nul().len(),
                    buffer.as_bytes()
                ));
                prop_assert!(ptr_range(
                    decoded.wide.as_ptr(),
                    decoded.wide.len(),
                    buffer.as_bytes()
                ));
                prop_assert!(ptr_range(
                    decoded.wide_c.as_ptr(),
                    decoded.wide_c.as_slice_with_nul().len(),
                    buffer.as_bytes()
                ));
                prop_assert!(ptr_range(
                    decoded.fixed.as_ptr(),
                    decoded.fixed.len(),
                    buffer.as_bytes()
                ));
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn floats_roundtrip_bit_exact_including_nan_and_signed_zero() {
    runner()
        .run(&(any::<u32>(), any::<u64>()), |(a, b)| {
            let value = Floats {
                f32_value: f32::from_bits(a),
                f64_value: f64::from_bits(b),
            };
            let mut buffer = zero_schema::make_buffer_for!(Floats);
            value.encode_into(buffer.as_bytes_mut()).unwrap();
            let decoded = Floats::parse(buffer.as_bytes()).unwrap();
            prop_assert_eq!(decoded.f32_value.to_bits(), a);
            prop_assert_eq!(decoded.f64_value.to_bits(), b);
            Ok(())
        })
        .unwrap();
}

#[test]
fn arbitrary_aligned_exact_bytes_never_panic_and_observations_are_stable() {
    runner()
        .run(
            &proptest::collection::vec(any::<u8>(), Logical::WIRE_SIZE),
            |bytes| {
                let mut buffer = zero_schema::make_buffer_for!(Logical<'static>);
                buffer.as_bytes_mut().copy_from_slice(&bytes);
                let first = std::panic::catch_unwind(|| Logical::parse(buffer.as_bytes()));
                prop_assert!(first.is_ok());
                let first = first.unwrap();
                let second = Logical::parse(buffer.as_bytes());
                match (first, second) {
                    (Ok(a), Ok(b)) => {
                        prop_assert_eq!(&a, &b);
                        prop_assert!(ptr_range(a.text.as_ptr(), a.text.len(), buffer.as_bytes()));
                    }
                    (Err(a), Err(b)) => {
                        prop_assert_eq!(a.kind(), b.kind());
                        prop_assert_eq!(a.schema(), b.schema());
                        prop_assert_eq!(a.segment(), b.segment());
                        prop_assert_eq!(a.validation_code(), b.validation_code());
                        prop_assert_eq!(a.to_string(), b.to_string());
                    }
                    _ => prop_assert!(false, "same bytes changed result"),
                }
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn selected_union_variant_and_inactive_tail_invariants() {
    runner()
        .run(&(any::<bool>(), any::<u32>()), |(data, raw)| {
            let value = if data {
                Choice::Data(Payload { value: raw })
            } else {
                Choice::Unit
            };
            let mut buffer = zero_schema::make_buffer_for!(Choice);
            value.encode_into(buffer.as_bytes_mut()).unwrap();
            prop_assert_eq!(Choice::parse(buffer.as_bytes()).unwrap(), value);
            if !data {
                let payload = match Choice::LAYOUT.kind() {
                    TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
                    _ => unreachable!(),
                };
                if Payload::WIRE_SIZE != 0 {
                    buffer.as_bytes_mut()[payload] = 1;
                    prop_assert_eq!(
                        Choice::parse(buffer.as_bytes()).unwrap_err().kind(),
                        ErrorKind::NonZeroTail
                    );
                }
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn zero_tail_and_padding_reject_the_first_corrupt_byte() {
    let strategy = (proptest::collection::vec(0x20u8..0x7f, 0..12), any::<u8>());
    runner()
        .run(&strategy, |(text_bytes, marker)| {
            let text = core::str::from_utf8(&text_bytes).unwrap();
            let ctext = c"ok";
            let wide = U16Str::from_slice(&[0x41]);
            let wide_c = U16CStr::from_slice(&[0x42, 0]).unwrap();
            let fixed = [marker; 5];
            let value = Logical {
                byte: marker,
                word: 7,
                signed: -9,
                flag: true,
                mode: Mode::Idle,
                child: Child {
                    signed: 11,
                    flag: false,
                },
                text,
                ctext,
                wide,
                wide_c,
                fixed: &fixed,
            };
            let mut buffer = zero_schema::make_buffer_for!(Logical<'static>);
            value.encode_into(buffer.as_bytes_mut()).unwrap();

            let text_field = Logical::LAYOUT
                .fields()
                .iter()
                .find(|field| field.name() == "text")
                .unwrap();
            let text_layout = match text_field.kind() {
                FieldKind::String(layout) => layout,
                _ => unreachable!(),
            };
            let first_tail = text_field.offset() + text_layout.data_offset() + text.len();
            prop_assert!(first_tail < text_field.offset() + text_field.size());
            buffer.as_bytes_mut()[first_tail] = 1;
            prop_assert_eq!(
                Logical::parse(buffer.as_bytes()).unwrap_err().kind(),
                ErrorKind::NonZeroTail
            );

            value.encode_into(buffer.as_bytes_mut()).unwrap();
            if let Some(range) = Logical::LAYOUT
                .padding()
                .iter()
                .find(|range| range.start() < range.end())
            {
                buffer.as_bytes_mut()[range.start()] = 1;
                prop_assert_eq!(
                    Logical::parse(buffer.as_bytes()).unwrap_err().kind(),
                    ErrorKind::NonZeroPadding
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn semantic_encode_failure_is_transactional() {
    runner()
        .run(
            &proptest::collection::vec(0x20u8..0x7f, 13..=32),
            |oversized| {
                let text = core::str::from_utf8(&oversized).unwrap();
                let fixed = [0u8; 5];
                let value = Logical {
                    byte: 1,
                    word: 2,
                    signed: 3,
                    flag: true,
                    mode: Mode::Active,
                    child: Child {
                        signed: 4,
                        flag: false,
                    },
                    text,
                    ctext: c"x",
                    wide: U16Str::from_slice(&[]),
                    wide_c: U16CStr::from_slice(&[0]).unwrap(),
                    fixed: &fixed,
                };
                let mut buffer = zero_schema::make_buffer_for!(Logical<'static>);
                buffer.as_bytes_mut().fill(0xa5);
                let before = buffer.as_bytes().to_vec();
                let error = value.encode_into(buffer.as_bytes_mut()).unwrap_err();
                prop_assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
                prop_assert_eq!(buffer.as_bytes(), before.as_slice());
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn regression_unknown_tag_precedes_corrupt_payload() {
    let mut buffer = zero_schema::make_buffer_for!(Choice);
    buffer.as_bytes_mut().fill(0xff);
    let error = Choice::parse(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnknownUnionTag);
}
