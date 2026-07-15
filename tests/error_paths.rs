#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::AllFeatures;
use zero_schema::{ErrorKind, ErrorPathSegment, SchemaError, error_path_string};

#[test]
fn eager_nested_access_errors_preserve_structured_field_index_and_variant_paths() {
    let mut fixture = producer::all_features_mut();
    fixture.as_bytes_mut()[72 + 5] = b'x';
    let error = AllFeatures::access(fixture.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingNul);
    assert_eq!(error_path_string(&error), "AllFeatures.headers[1].producer");
    let child = error.child().unwrap();
    assert_eq!(child.segment(), Some(ErrorPathSegment::Index(1)));
    assert_eq!(
        child.child().unwrap().segment(),
        Some(ErrorPathSegment::Field("producer"))
    );

    let mut fixture = producer::all_features_mut();
    fixture.as_bytes_mut()[producer::all_features_offsets::CONFIG + 2] = 2;
    let error = AllFeatures::access(fixture.as_bytes()).unwrap_err();
    assert_eq!(
        error_path_string(&error),
        "AllFeatures.config.Memory.enabled"
    );
    assert_eq!(
        format!("{error}").split_once(": ").unwrap().0,
        "AllFeatures.config.Memory.enabled"
    );
}
