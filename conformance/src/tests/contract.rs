use crate::BUILD_CASES;
use crate::ffi::{cpp_inspect_fixture, cpp_layout_report, cpp_write_fixture};
use crate::inventory::CASES;
use std::collections::BTreeSet;

#[test]
fn frozen_ids_roots_and_keys_match_all_three_sources() {
    const ROOTS: [&str; 11] = [
        "conformance-scalars",
        "conformance-aligned",
        "conformance-message-empty",
        "conformance-message-data",
        "conformance-primitives",
        "conformance-enums",
        "conformance-strings",
        "conformance-nested",
        "conformance-zst-layout",
        "conformance-external-message",
        "conformance-external-units",
    ];
    assert_eq!(CASES.len(), 11);
    assert_eq!(BUILD_CASES.len(), 11);
    let mut all_keys = BTreeSet::new();
    for (index, (case, build)) in CASES.iter().zip(BUILD_CASES).enumerate() {
        let id = 1001 + index as u32;
        assert_eq!(case.case_id, id);
        assert_eq!(build.case_id, id);
        assert_eq!(case.root_id, ROOTS[index]);
        assert_eq!(build.root_id, ROOTS[index]);

        let expected = (case.expected_key_values)().expect("Rust layout contract");
        let expected_keys: Vec<_> = expected.iter().map(|&(key, _)| key).collect();
        assert_eq!(expected_keys, build.layout_keys);
        assert!(build.layout_keys.windows(2).all(|keys| keys[0] < keys[1]));
        assert!(
            build
                .observation_keys
                .windows(2)
                .all(|keys| keys[0] < keys[1])
        );
        for &key in build.layout_keys.iter().chain(build.observation_keys) {
            assert_ne!(key, 0);
            assert!(all_keys.insert(key), "duplicate key {key}");
        }
    }
}

#[test]
fn every_case_agrees_in_both_codec_directions() {
    for (case, _build) in CASES.iter().zip(BUILD_CASES) {
        let expected_layout = (case.expected_key_values)().expect("Rust layout contract");
        let cpp_layout = cpp_layout_report(case.case_id)
            .unwrap_or_else(|error| panic!("C++ layout report case {}: {error}", case.case_id));
        assert_eq!(
            cpp_layout.pairs(),
            expected_layout.as_slice(),
            "layout {}",
            case.case_id
        );

        let rust_bytes = (case.rust_bytes)().expect("Rust fixture");
        let cpp_observed = cpp_inspect_fixture(case.case_id, &rust_bytes)
            .unwrap_or_else(|error| panic!("C++ inspection case {}: {error}", case.case_id));
        let rust_observed = (case.rust_observe)(&rust_bytes).expect("Rust observation");
        assert_eq!(
            cpp_observed.pairs(),
            rust_observed.as_slice(),
            "Rust to C++ {}",
            case.case_id
        );

        let cpp_bytes = cpp_write_fixture(case.case_id, rust_bytes.len())
            .unwrap_or_else(|error| panic!("C++ write case {}: {error}", case.case_id));
        assert_eq!(cpp_bytes, rust_bytes, "whole bytes {}", case.case_id);
        assert_eq!(
            (case.rust_observe)(&cpp_bytes).expect("Rust inspection"),
            rust_observed
        );
    }
}

#[test]
fn cpp_inspection_accepts_unaligned_input() {
    for (case, _build) in CASES.iter().zip(BUILD_CASES) {
        let bytes = (case.rust_bytes)().expect("Rust fixture");
        let mut storage = vec![0x5a; bytes.len() + 1];
        storage[1..].copy_from_slice(&bytes);
        let report = cpp_inspect_fixture(case.case_id, &storage[1..])
            .expect("unaligned input must be accepted");
        assert_eq!(
            report.pairs(),
            (case.rust_observe)(&bytes).unwrap().as_slice()
        );
        assert_eq!(storage[0], 0x5a);
    }
}

#[test]
fn external_units_parent_and_child_layout_witness() {
    use zero_schema::TypeKind;
    use zero_schema_schema_corpus::conformance::{ConformanceExternalUnits, ConformanceUnits};

    let parent = ConformanceExternalUnits::LAYOUT;
    let payload = parent
        .fields()
        .iter()
        .find(|field| field.name() == "payload")
        .unwrap();
    let TypeKind::TaggedUnion {
        payload_size,
        payload_align,
        ..
    } = ConformanceUnits::LAYOUT.kind()
    else {
        panic!("expected tagged union")
    };
    assert_eq!(
        (payload.offset(), payload.size(), payload.align()),
        (8, 0, 8)
    );
    assert_eq!((payload_size, payload_align), (0, 1));
}
