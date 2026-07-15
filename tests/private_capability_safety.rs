#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{
    AllFeatures, AllFeaturesAccessError, AllFeaturesMut, AllFeaturesMutationError,
    AllFeaturesPatch, AllFeaturesRef,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use zero_schema::{ErrorKind, SchemaError};

fn assert_generated_roots_reject_without_panicking(
    offset: usize,
    value: u8,
    expected_kind: ErrorKind,
) {
    let mut shared_fixture = producer::all_features_mut();
    shared_fixture.as_bytes_mut()[offset] = value;
    let shared_before = shared_fixture.as_bytes().to_vec();
    let shared = catch_unwind(AssertUnwindSafe(|| {
        AllFeatures::access(shared_fixture.as_bytes())
    }));
    let shared_error =
        match shared.expect("generated shared access must not panic on invalid bytes") {
            Ok(_) => panic!("generated shared access minted a capability from invalid bytes"),
            Err(error) => error,
        };
    assert_eq!(shared_error.kind(), expected_kind);
    assert_eq!(shared_fixture.as_bytes(), shared_before);

    let mut mutable_fixture = producer::all_features_mut();
    mutable_fixture.as_bytes_mut()[offset] = value;
    let mutable_before = mutable_fixture.as_bytes().to_vec();
    let mutable = catch_unwind(AssertUnwindSafe(|| {
        AllFeatures::access_mut(mutable_fixture.as_bytes_mut()).map(|_| ())
    }));
    let mutable_error =
        match mutable.expect("generated exclusive access must not panic on invalid bytes") {
            Ok(_) => panic!("generated exclusive access minted a capability from invalid bytes"),
            Err(error) => error,
        };
    assert_eq!(mutable_error.kind(), expected_kind);
    assert_eq!(mutable_fixture.as_bytes(), mutable_before);
}

#[test]
fn generated_roots_reject_invalid_exact_inputs_without_mutating_them() {
    for (offset, value, kind) in [
        (
            producer::all_features_offsets::ACTIVE,
            2,
            ErrorKind::InvalidBool,
        ),
        (
            producer::all_features_offsets::PRIORITY,
            99,
            ErrorKind::UnknownEnumValue,
        ),
        (
            producer::all_features_offsets::NAME,
            8,
            ErrorKind::LengthOutOfBounds,
        ),
        (
            producer::all_features_offsets::CONFIG_KIND,
            3,
            ErrorKind::UnknownUnionTag,
        ),
        (
            producer::all_features_offsets::CONFIG + 2,
            2,
            ErrorKind::InvalidBool,
        ),
    ] {
        assert_generated_roots_reject_without_panicking(offset, value, kind);
    }
}

#[test]
fn generated_roots_compose_only_after_validating_the_exact_input() {
    let fixture = producer::all_features_mut();
    let view = AllFeatures::access(fixture.as_bytes())
        .expect("reviewed producer bytes satisfy the generated proof");
    assert_eq!(
        view.copy_into(),
        AllFeatures::access(fixture.as_bytes()).unwrap().copy_into()
    );

    let mut fixture = producer::all_features_mut();
    {
        let mut view = AllFeatures::access_mut(fixture.as_bytes_mut())
            .expect("reviewed producer bytes satisfy the generated exclusive proof");
        view.sequence_mut().set(43).unwrap();
    }
    assert_eq!(
        AllFeatures::access(fixture.as_bytes()).unwrap().sequence(),
        43
    );
}

#[test]
fn public_root_surface_is_capability_only_and_uses_final_names() {
    let mut fixture = producer::all_features_mut();
    let _: Result<AllFeaturesRef<'_>, AllFeaturesAccessError> =
        AllFeatures::access(fixture.as_bytes());
    let _: (usize, usize, usize) = (
        AllFeatures::SCHEMA_SIZE,
        AllFeatures::SCHEMA_ALIGN,
        AllFeatures::SCHEMA_STRIDE,
    );
    let _ = AllFeatures::LAYOUT;

    let mut view: AllFeaturesMut<'_> = AllFeatures::access_mut(fixture.as_bytes_mut())
        .expect("reviewed producer bytes satisfy the generated exclusive proof");
    let _ = view.copy_into();
    let patch: AllFeaturesPatch<'_> = Default::default();
    let _: Result<(), AllFeaturesMutationError> = view.copy_from(&patch);
}
