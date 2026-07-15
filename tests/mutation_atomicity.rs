#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, AllFeaturesPatch, ConfigKind};
use zero_schema::{ErrorKind, ErrorPathSegment, SchemaError};

fn snapshot(bytes: &producer::AlignedAllFeatures) -> [u8; producer::ALL_FEATURES_LEN] {
    bytes.as_bytes().try_into().unwrap()
}

#[test]
fn failed_field_and_array_preflight_preserves_all_producer_bytes() {
    let mut fixture = producer::all_features_mut();
    let before = snapshot(&fixture);
    let error = AllFeatures::access_mut(fixture.as_bytes_mut())
        .unwrap()
        .name_mut()
        .set("overlong")
        .unwrap_err();
    assert_eq!(
        (error.kind(), error.segment()),
        (
            ErrorKind::CapacityExceeded,
            Some(ErrorPathSegment::Field("name"))
        )
    );
    assert_eq!(fixture.as_bytes(), before);

    let mut fixture = producer::all_features_mut();
    let before = snapshot(&fixture);
    let error = AllFeatures::access_mut(fixture.as_bytes_mut())
        .unwrap()
        .samples_mut()
        .copy_from(&[1, 2])
        .unwrap_err();
    assert_eq!(
        (error.kind(), error.segment()),
        (
            ErrorKind::ArrayLengthMismatch,
            Some(ErrorPathSegment::Field("samples"))
        )
    );
    assert_eq!(fixture.as_bytes(), before);

    let mut fixture = producer::all_features_mut();
    let before = snapshot(&fixture);
    let error = AllFeatures::access_mut(fixture.as_bytes_mut())
        .unwrap()
        .samples_mut()
        .set(3, 99)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ArrayIndexOutOfBounds);
    assert_eq!(fixture.as_bytes(), before);
}

#[test]
fn tag_only_and_mismatched_union_patches_are_atomic() {
    let mut fixture = producer::all_features_mut();
    let before = snapshot(&fixture);
    let error = AllFeatures::access_mut(fixture.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: None,
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TagOnlyPatch);
    assert_eq!(fixture.as_bytes(), before);
}
