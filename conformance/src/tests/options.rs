use zero_schema::{FieldDescriptor, FieldKind};
use zero_schema_schema_corpus::conformance::{
    ConformanceOptionChild, ConformanceOptionKind, ConformanceOptions,
};

use crate::ffi::{cpp_inspect_fixture, cpp_layout_report, cpp_write_fixture};
use crate::inventory::{native_option_clear_child, native_option_layout, native_option_observe};

const NATIVE_CASE_IDS: [u32; 5] = [1012, 1013, 1014, 1015, 1016];

fn option_field(name: &str) -> &'static FieldDescriptor {
    ConformanceOptions::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("known optional conformance field")
}

fn fixture(case_id: u32) -> Vec<u8> {
    cpp_write_fixture(case_id, ConformanceOptions::SCHEMA_SIZE)
        .unwrap_or_else(|error| panic!("C++ native option fixture {case_id}: {error}"))
}

fn expected(case_id: u32, values: [u64; 3]) -> [(u64, u64); 3] {
    let key = u64::from(case_id) * 1000 + 501;
    [(key, values[0]), (key + 1, values[1]), (key + 2, values[2])]
}

fn assert_shared_observation(case_id: u32, bytes: &[u8], values: [u64; 3]) {
    let rust = native_option_observe(case_id, bytes)
        .unwrap_or_else(|error| panic!("Rust native option observation {case_id}: {error}"));
    let cpp = cpp_inspect_fixture(case_id, bytes)
        .unwrap_or_else(|error| panic!("C++ native option observation {case_id}: {error}"));
    let expected = expected(case_id, values);
    assert_eq!(rust.as_slice(), expected);
    assert_eq!(cpp.pairs(), expected);
}

fn assert_declared_payloads(case_id: u32, bytes: &[u8]) {
    let mut storage = zero_schema::make_schema_buffer!(ConformanceOptions);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceOptions::access(storage.as_bytes())
        .unwrap_or_else(|_| panic!("declared native option payload {case_id} is valid"));
    match case_id {
        1012 => {
            assert!(view.maybe_kind().is_none());
            assert!(view.maybe_child().is_none());
            assert!(view.maybe_array().is_none());
        }
        1013 => {
            assert_eq!(view.maybe_kind(), Some(ConformanceOptionKind::One));
            assert!(view.maybe_child().is_none());
            assert!(view.maybe_array().is_none());
        }
        1014 => {
            assert!(view.maybe_kind().is_none());
            let child = view.maybe_child().expect("declared child payload");
            assert_eq!(child.first(), ConformanceOptionKind::Two);
            assert_eq!(child.second(), ConformanceOptionKind::One);
            assert!(view.maybe_array().is_none());
        }
        1015 => {
            assert!(view.maybe_kind().is_none());
            assert!(view.maybe_child().is_none());
            let array = view.maybe_array().expect("declared array payload");
            assert_eq!(array.get(0), Some(ConformanceOptionKind::One));
            assert_eq!(array.get(1), Some(ConformanceOptionKind::Two));
        }
        1016 => {
            assert_eq!(view.maybe_kind(), Some(ConformanceOptionKind::Two));
            let child = view.maybe_child().expect("declared child payload");
            assert_eq!(child.first(), ConformanceOptionKind::One);
            assert_eq!(child.second(), ConformanceOptionKind::Two);
            let array = view.maybe_array().expect("declared array payload");
            assert_eq!(array.get(0), Some(ConformanceOptionKind::Two));
            assert_eq!(array.get(1), Some(ConformanceOptionKind::One));
        }
        _ => panic!("unknown native option case {case_id}"),
    }
}

fn assert_rejected_by_both(case_id: u32, bytes: &[u8]) {
    assert!(
        native_option_observe(case_id, bytes).is_err(),
        "Rust must eagerly reject malformed optional storage"
    );
    assert!(
        cpp_inspect_fixture(case_id, bytes).is_err(),
        "C++ must eagerly reject malformed optional storage"
    );
}

#[test]
fn native_option_layout_reports_exact_storage_and_metadata() {
    let reference = native_option_layout().expect("Rust optional layout metadata");
    for case_id in NATIVE_CASE_IDS {
        let key_offset = u64::from(case_id) * 1000 - 1_012_000;
        let expected: Vec<_> = reference
            .iter()
            .map(|&(key, value)| (key + key_offset, value))
            .collect();
        assert_eq!(
            cpp_layout_report(case_id)
                .unwrap_or_else(|error| panic!("C++ optional layout report {case_id}: {error}"))
                .pairs(),
            expected.as_slice()
        );
    }

    let kind = option_field("maybe_kind");
    let child = option_field("maybe_child");
    let array = option_field("maybe_array");
    assert!(kind.is_optional());
    assert!(child.is_optional());
    assert!(array.is_optional());

    let FieldKind::ScalarEnum {
        layout: kind_layout,
    } = kind.kind()
    else {
        panic!("optional enum keeps scalar-enum metadata");
    };
    assert_eq!(kind_layout.size(), 1);
    assert!(
        kind.size() > kind_layout.size(),
        "field alignment padding is part of the optional sentinel span"
    );

    let FieldKind::Schema {
        layout: child_layout,
    } = child.kind()
    else {
        panic!("optional nested record keeps schema metadata");
    };
    assert_eq!(child_layout, ConformanceOptionChild::LAYOUT);
    assert_eq!(child.size(), ConformanceOptionChild::SCHEMA_SIZE);

    let FieldKind::Array(array_layout) = array.kind() else {
        panic!("optional fixed array keeps array metadata");
    };
    assert_eq!(array_layout.length(), 2);
    assert_eq!(array_layout.stride(), kind_layout.size());
    assert_eq!(array.size(), array_layout.length() * array_layout.stride());
}

#[test]
fn native_cxx_producer_covers_none_individual_some_and_all_some() {
    let cases = [
        (1012, [0, 0, 0]),
        (1013, [1, 0, 0]),
        (1014, [0, 1, 0]),
        (1015, [0, 0, 1]),
        (1016, [1, 1, 1]),
    ];
    for (case_id, values) in cases {
        let bytes = fixture(case_id);
        assert_eq!(
            bytes,
            fixture(case_id),
            "native fixture must be deterministic"
        );
        assert_shared_observation(case_id, &bytes, values);
        assert_declared_payloads(case_id, &bytes);
    }

    let bytes = fixture(1012);
    for name in ["maybe_kind", "maybe_child", "maybe_array"] {
        let field = option_field(name);
        assert!(
            bytes[field.offset()..field.offset() + field.size()]
                .iter()
                .all(|byte| *byte == 0),
            "C++ None producer clears the complete {name} field span"
        );
    }
}

#[test]
fn nonzero_internal_padding_is_valid_for_present_nested_option() {
    let mut bytes = fixture(1014);
    let child = option_field("maybe_child");
    let padding = ConformanceOptionChild::LAYOUT
        .padding()
        .iter()
        .copied()
        .find(|range| range.start() < range.end())
        .expect("nested option child has internal padding");
    assert!(padding.end() <= child.size());
    bytes[child.offset() + padding.start()..child.offset() + padding.end()].fill(0xa5);

    assert_shared_observation(1014, &bytes, [0, 1, 0]);
    let mut storage = zero_schema::make_schema_buffer!(ConformanceOptions);
    storage.as_bytes_mut().copy_from_slice(&bytes);
    let view = ConformanceOptions::access(storage.as_bytes())
        .expect("nonzero internal child padding remains a valid Some");
    let child = view.maybe_child().expect("child remains present");
    assert_eq!(child.first(), ConformanceOptionKind::Two);
    assert_eq!(child.second(), ConformanceOptionKind::One);
}

#[test]
fn parent_padding_is_excluded_from_optional_sentinel_scans() {
    let mut bytes = fixture(1012);
    let parent_padding = ConformanceOptions::LAYOUT
        .padding()
        .iter()
        .copied()
        .find(|range| {
            range.start() < range.end()
                && ConformanceOptions::LAYOUT.fields().iter().all(|field| {
                    range.end() <= field.offset() || range.start() >= field.offset() + field.size()
                })
        })
        .expect("root has parent-only padding before aligned optional storage");
    bytes[parent_padding.start()..parent_padding.end()].fill(0x96);
    assert_shared_observation(1012, &bytes, [0, 0, 0]);
}

#[test]
fn every_nonzero_byte_in_optional_storage_matches_rust_and_cxx() {
    let field = option_field("maybe_kind");
    let FieldKind::ScalarEnum { layout } = field.kind() else {
        panic!("optional enum metadata");
    };
    assert_eq!(layout.size(), 1);
    assert_eq!(field.size(), 8, "aligned enum storage span");
    let original = fixture(1012);
    for index in 0..field.size() {
        let mut bytes = original.clone();
        bytes[field.offset() + index] = 0x7e;
        let rust = native_option_observe(1012, &bytes);
        let cpp = cpp_inspect_fixture(1012, &bytes);
        assert_eq!(
            rust.is_ok(),
            cpp.is_ok(),
            "Rust/C++ optional result differs for storage byte {index}"
        );
        assert!(
            rust.is_err(),
            "nonzero storage byte {index} must be a malformed present enum"
        );
    }
}

#[test]
fn optional_nested_record_rejects_nonzero_padding_with_zero_required_fields() {
    let field = option_field("maybe_child");
    let FieldKind::Schema { layout } = field.kind() else {
        panic!("optional child metadata");
    };
    assert_eq!(layout, ConformanceOptionChild::LAYOUT);
    let padding = layout
        .padding()
        .iter()
        .copied()
        .find(|range| range.start() < range.end())
        .expect("nested child has padding");
    let mut bytes = fixture(1012);
    bytes[field.offset() + padding.start()] = 0x7e;
    assert_rejected_by_both(1012, &bytes);
}

#[test]
fn optional_array_rejects_a_later_zero_invalid_element() {
    let field = option_field("maybe_array");
    let FieldKind::Array(array) = field.kind() else {
        panic!("optional array metadata");
    };
    assert_eq!(array.length(), 2);
    let mut bytes = fixture(1012);
    bytes[field.offset()] = ConformanceOptionKind::One as u8;
    bytes[field.offset() + array.stride()] = 0;
    assert_rejected_by_both(1012, &bytes);
}

#[test]
fn rust_clear_zeroes_the_complete_option_span_for_cxx_observation() {
    let cleared = native_option_clear_child(&fixture(1016)).expect("Rust optional clear");
    let child = option_field("maybe_child");
    assert!(
        cleared[child.offset()..child.offset() + child.size()]
            .iter()
            .all(|byte| *byte == 0),
        "Rust clear must zero the complete C++ sentinel span"
    );
    assert_shared_observation(1016, &cleared, [1, 0, 1]);
}

#[test]
fn native_option_case_block_is_complete_and_contiguous() {
    assert_eq!(NATIVE_CASE_IDS, [1012, 1013, 1014, 1015, 1016]);
}
