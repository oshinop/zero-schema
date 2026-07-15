use crate::BUILD_CASES;
use crate::ffi::{cpp_inspect_fixture, cpp_layout_report, cpp_write_fixture};
use crate::inventory::CASES;
use std::collections::BTreeSet;
use zero_schema::FieldKind;
use zero_schema_schema_corpus::conformance::ConformanceStrings;

const CASE_IDS: [u32; 10] = [1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1010, 1011];
const ROOTS: [&str; 10] = [
    "conformance-scalars",
    "conformance-aligned",
    "conformance-message-empty",
    "conformance-message-data",
    "conformance-primitives",
    "conformance-enums",
    "conformance-strings",
    "conformance-nested",
    "conformance-external-message",
    "conformance-external-units",
];
const NATIVE_CASE_IDS: [u32; 5] = [1012, 1013, 1014, 1015, 1016];
const NATIVE_ROOTS: [&str; 5] = [
    "conformance-options-none",
    "conformance-options-kind",
    "conformance-options-child",
    "conformance-options-array",
    "conformance-options-all",
];

#[test]
fn frozen_ids_roots_and_keys_match_all_three_sources() {
    assert_eq!(CASES.len(), CASE_IDS.len());
    assert_eq!(BUILD_CASES.len(), CASE_IDS.len() + NATIVE_CASE_IDS.len());
    let mut all_keys = BTreeSet::new();
    for ((case, build), (&id, root)) in CASES
        .iter()
        .zip(BUILD_CASES.iter().take(CASE_IDS.len()))
        .zip(CASE_IDS.iter().zip(ROOTS))
    {
        assert_eq!(case.case_id, id);
        assert_eq!(build.case_id, id);
        assert_eq!(case.root_id, root);
        assert_eq!(build.root_id, root);

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
    let native: Vec<_> = BUILD_CASES.iter().skip(CASE_IDS.len()).collect();
    assert_eq!(
        native.iter().map(|case| case.case_id).collect::<Vec<_>>(),
        NATIVE_CASE_IDS
    );
    assert_eq!(
        native.iter().map(|case| case.root_id).collect::<Vec<_>>(),
        NATIVE_ROOTS
    );
}

#[test]
fn cxx_producer_rust_capabilities_and_cxx_observation_agree() {
    for case in CASES {
        let expected_layout = (case.expected_key_values)().expect("Rust layout contract");
        let cpp_layout = cpp_layout_report(case.case_id)
            .unwrap_or_else(|error| panic!("C++ layout report case {}: {error}", case.case_id));
        assert_eq!(
            cpp_layout.pairs(),
            expected_layout.as_slice(),
            "layout {}",
            case.case_id
        );

        let producer_bytes = cpp_write_fixture(case.case_id, (case.schema_size)())
            .unwrap_or_else(|error| panic!("C++ producer case {}: {error}", case.case_id));
        let rust_observed = (case.rust_observe)(&producer_bytes)
            .unwrap_or_else(|error| panic!("Rust access case {}: {error}", case.case_id));
        let cpp_observed = cpp_inspect_fixture(case.case_id, &producer_bytes)
            .unwrap_or_else(|error| panic!("C++ inspection case {}: {error}", case.case_id));
        assert_eq!(
            cpp_observed.pairs(),
            rust_observed.as_slice(),
            "C++ producer observation {}",
            case.case_id
        );

        if let Some(mutate) = case.rust_mutate {
            let mutated = mutate(&producer_bytes).unwrap_or_else(|error| {
                panic!("Rust constrained mutation case {}: {error}", case.case_id)
            });
            let inspection = cpp_inspect_fixture(case.case_id, &mutated).unwrap_or_else(|error| {
                panic!("C++ mutation inspection case {}: {error}", case.case_id)
            });
            assert_eq!(
                inspection.pairs(),
                case.mutated_observations,
                "Rust mutation C++ inspection {}",
                case.case_id
            );
        }
    }
}
#[test]
fn cxx_inspects_rust_selected_payload_mutation() {
    let case = CASES
        .iter()
        .find(|case| case.case_id == 1010)
        .expect("external data case");
    let producer_bytes =
        cpp_write_fixture(case.case_id, (case.schema_size)()).expect("C++ producer");
    let mutate = case
        .rust_mutate
        .expect("external data case must exercise constrained Rust mutation");
    let mutated = mutate(&producer_bytes).expect("Rust selected-payload mutation");
    let observed =
        cpp_inspect_fixture(case.case_id, &mutated).expect("C++ inspection of Rust mutation");
    assert_eq!(observed.pairs(), case.mutated_observations);
    assert_eq!(
        observed.pairs()[1..3],
        [(1010502, 11), (1010503, 11)],
        "the selected-payload mutation leaves both external tag observations intact"
    );
}

#[test]
fn cpp_observation_accepts_unaligned_input() {
    for case in CASES {
        let bytes = cpp_write_fixture(case.case_id, (case.schema_size)()).expect("C++ producer");
        let expected = cpp_inspect_fixture(case.case_id, &bytes).expect("aligned C++ observation");
        let mut storage = vec![0x5a; bytes.len() + 1];
        storage[1..].copy_from_slice(&bytes);
        let actual = cpp_inspect_fixture(case.case_id, &storage[1..])
            .expect("C++ byte observer accepts unaligned caller memory");
        assert_eq!(actual, expected);
        assert_eq!(storage[0], 0x5a);
    }
}

#[test]
fn unit_payloads_use_nonzero_private_union_members() {
    let case_id = 1011;
    let layout = CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .expect("unit case")
        .expected_key_values;
    let pairs = layout().expect("unit layout");
    assert!(
        pairs
            .iter()
            .any(|&(key, value)| key == 1011321 && value == 1)
    );
    assert!(
        pairs
            .iter()
            .any(|&(key, value)| key == 1011324 && value == 1)
    );
}

fn layout_value(case_id: u32, key: u64) -> usize {
    let case = CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .expect("known conformance case");
    let (_, value) = (case.expected_key_values)()
        .expect("Rust layout")
        .into_iter()
        .find(|(actual, _)| *actual == key)
        .expect("known layout key");
    usize::try_from(value).expect("layout value fits usize")
}

fn assert_ignored_bytes(case_id: u32, bytes: &[u8], expected: &[(u64, u64)]) {
    let case = CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .expect("known conformance case");
    assert_eq!(
        (case.rust_observe)(bytes).expect("Rust ignores unrelated bytes"),
        expected
    );
    assert_eq!(
        cpp_inspect_fixture(case_id, bytes)
            .expect("C++ ignores unrelated bytes")
            .pairs(),
        expected
    );
}

#[test]
fn padding_unused_capacity_and_inactive_payload_bytes_are_ignored() {
    let case = CASES.iter().find(|case| case.case_id == 1002).unwrap();
    let mut bytes = cpp_write_fixture(1002, (case.schema_size)()).expect("C++ producer");
    let expected = (case.rust_observe)(&bytes).expect("Rust access");
    let prefix_end = layout_value(1002, 1002010) + layout_value(1002, 1002011);
    let value_offset = layout_value(1002, 1002013);
    assert!(
        prefix_end < value_offset,
        "aligned record must contain padding"
    );
    bytes[prefix_end..value_offset].fill(0xa5);
    assert_ignored_bytes(1002, &bytes, &expected);

    let case = CASES.iter().find(|case| case.case_id == 1007).unwrap();
    let mut bytes = cpp_write_fixture(1007, (case.schema_size)()).expect("C++ producer");
    let expected = (case.rust_observe)(&bytes).expect("Rust access");
    let field = ConformanceStrings::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "utf8_u16_native")
        .expect("string field");
    let FieldKind::String(string) = field.kind() else {
        panic!("string metadata")
    };
    let start = field.offset() + string.data_offset() + 1;
    let end = field.offset() + field.size();
    assert!(start < end, "fixture must have unused string capacity");
    bytes[start..end].fill(0xb6);
    assert_ignored_bytes(1007, &bytes, &expected);

    let case = CASES.iter().find(|case| case.case_id == 1003).unwrap();
    let mut bytes = cpp_write_fixture(1003, (case.schema_size)()).expect("C++ producer");
    let expected = (case.rust_observe)(&bytes).expect("Rust access");
    let payload_offset = layout_value(1003, 1003013);
    let payload_size = layout_value(1003, 1003014);
    assert_ne!(
        payload_size, 0,
        "unit payload union has a private nonzero form"
    );
    bytes[payload_offset..payload_offset + payload_size].fill(0xc7);
    assert_ignored_bytes(1003, &bytes, &expected);
}
