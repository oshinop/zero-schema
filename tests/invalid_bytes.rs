#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, ConfigKind};
use zero_schema::{ErrorKind, ErrorPathSegment, SchemaError};

fn assert_invalid(
    mut fixture: producer::AlignedAllFeatures,
    offset: usize,
    value: u8,
    kind: ErrorKind,
    path: ErrorPathSegment,
) {
    fixture.as_bytes_mut()[offset] = value;
    let error = AllFeatures::access(fixture.as_bytes()).unwrap_err();
    assert_eq!((error.kind(), error.segment()), (kind, Some(path)));
}

#[test]
fn access_eagerly_rejects_type_invalid_producer_storage_in_declaration_order() {
    assert_invalid(
        producer::all_features_mut(),
        producer::all_features_offsets::ACTIVE,
        2,
        ErrorKind::InvalidBool,
        ErrorPathSegment::Field("active"),
    );
    assert_invalid(
        producer::all_features_mut(),
        producer::all_features_offsets::PRIORITY,
        99,
        ErrorKind::UnknownEnumValue,
        ErrorPathSegment::Field("priority"),
    );
    assert_invalid(
        producer::all_features_mut(),
        producer::all_features_offsets::NAME,
        8,
        ErrorKind::LengthOutOfBounds,
        ErrorPathSegment::Field("name"),
    );
    assert_invalid(
        producer::all_features_mut(),
        producer::all_features_offsets::NAME + 1,
        0xff,
        ErrorKind::InvalidUtf8,
        ErrorPathSegment::Field("name"),
    );
    assert_invalid(
        producer::all_features_mut(),
        producer::all_features_offsets::CONFIG_KIND,
        ConfigKind::Reserved as u8,
        ErrorKind::UnknownUnionTag,
        ErrorPathSegment::Field("config"),
    );

    let mut missing_nul = producer::all_features_mut();
    missing_nul.as_bytes_mut()
        [producer::all_features_offsets::C_NAME..producer::all_features_offsets::WIDE]
        .fill(b'x');
    let error = AllFeatures::access(missing_nul.as_bytes()).unwrap_err();
    assert_eq!(
        (error.kind(), error.segment()),
        (
            ErrorKind::MissingNul,
            Some(ErrorPathSegment::Field("c_name"))
        )
    );

    let mut selected_payload = producer::all_features_mut();
    selected_payload.as_bytes_mut()[producer::all_features_offsets::CONFIG + 2] = 2;
    let error = AllFeatures::access(selected_payload.as_bytes()).unwrap_err();
    assert_eq!(
        (error.kind(), error.segment()),
        (
            ErrorKind::InvalidBool,
            Some(ErrorPathSegment::Field("config"))
        )
    );
    assert_eq!(
        error.child().unwrap().segment(),
        Some(ErrorPathSegment::Variant("Memory"))
    );
}

#[test]
fn ignored_padding_unused_capacity_and_inactive_payload_do_not_participate_in_proof() {
    let baseline = producer::all_features_mut();
    let expected = AllFeatures::access(baseline.as_bytes())
        .unwrap()
        .copy_into();
    let mut altered = producer::all_features_mut();
    for &(start, end) in producer::all_features_offsets::PADDING {
        altered.as_bytes_mut()[start..end].fill(0xa5);
    }
    for &(start, end) in producer::all_features_offsets::UNUSED_CAPACITY {
        altered.as_bytes_mut()[start..end].fill(0xb6);
    }
    let (start, end) = producer::all_features_offsets::INACTIVE_UNION;
    altered.as_bytes_mut()[start..end].fill(0xc7);
    assert_eq!(
        AllFeatures::access(altered.as_bytes()).unwrap().copy_into(),
        expected
    );
}
