use zero_schema::{ErrorKind, ErrorPathSegment, FieldKind, TypeKind};
use zero_schema_cross_crate_child::{
    BigCode, BorrowedChild, DirectChild, GenericBytes, OptionalChild, OptionalCode,
    TrailingProjection,
};
use zero_schema_cross_crate_consumer::{
    CompositionParent, OptionalParent, PublicParent, child_message_unknown_facts,
    composition_from_fixture, composition_metadata, composition_parent_fixture, generic_layout,
    optional_parent_mutation_and_patch, optional_parent_none_from_zeroed, private_error_facts,
    private_parent_fixture, private_parent_from_fixture, public_parent_fixture,
    public_parent_from_fixture, truly_private_error_facts,
};

#[repr(align(16))]
struct Aligned<const N: usize>([u8; N]);

#[test]
fn public_child_capabilities_cross_the_crate_boundary() {
    let big = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-child/golden/big-ready.bin"
    ));
    assert_eq!(BigCode::access(&big.0).unwrap().get(), BigCode::Ready);
    assert_eq!(DirectChild::SCHEMA_SIZE, 4);
    assert_eq!(BorrowedChild::<'static>::SCHEMA_ALIGN, 4);
    assert_eq!(GenericBytes::<'static, 3>::SCHEMA_SIZE, 3);
}

#[test]
fn public_child_materializes_through_public_parent_without_exposing_support() {
    assert_eq!(
        public_parent_from_fixture(),
        zero_schema_cross_crate_consumer::Observation {
            prefix: 7,
            valid: true
        }
    );

    let bytes = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-consumer/golden/public-parent.bin"
    ));
    let view = PublicParent::access(&bytes.0).unwrap();
    let _ = view.copy_into();
    assert_eq!(PublicParent::SCHEMA_SIZE, public_parent_fixture().len());
    assert_eq!(PublicParent::LAYOUT.name(), "PublicParent");
    assert_eq!(PublicParent::LAYOUT.kind(), TypeKind::Struct);
    match PublicParent::LAYOUT.fields()[1].kind() {
        FieldKind::Schema { layout } => assert_eq!(layout.name(), "DirectChild"),
        other => panic!("unexpected child descriptor: {other:?}"),
    }
}

#[test]
fn genuinely_private_child_is_usable_behind_public_helpers() {
    assert_eq!(
        private_parent_from_fixture(),
        zero_schema_cross_crate_consumer::Observation {
            prefix: 11,
            valid: false,
        }
    );
    assert_eq!(private_parent_fixture().len(), 6);

    let mut malformed = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-consumer/golden/private-parent.bin"
    ));
    malformed.0[2] = 2;
    let error = truly_private_error_facts(&malformed.0);
    assert_eq!(error.kind, ErrorKind::InvalidBool);
    assert_eq!(error.segment, Some(ErrorPathSegment::Field("child")));
    assert_eq!(error.child_schema, Some("PrivateChild"));
    assert!(error.child_source_identical);
}

#[test]
fn external_tagged_payload_metadata_and_diagnostics_cross_crate_boundaries() {
    let (size, align, tag_offset, payload_offset, payload_name) = composition_metadata();
    assert_eq!(
        (size, align, tag_offset, payload_offset, payload_name),
        (16, 4, 10, 12, "tagged")
    );
    let field = &CompositionParent::LAYOUT.fields()[3];
    assert!(matches!(
        field.kind(),
        FieldKind::ExternalTaggedUnion { .. }
    ));

    let mut bytes = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-consumer/golden/composition-parent.bin"
    ));
    bytes.0[tag_offset] = 3;
    let tagged = child_message_unknown_facts(&bytes.0);
    assert_eq!(tagged.kind, ErrorKind::UnknownUnionTag);
    assert_eq!(tagged.schema, "CompositionParent");
    assert_eq!(tagged.segment, Some(ErrorPathSegment::Field("tagged")));
    assert_eq!(tagged.child_schema, Some("ChildMessage"));
    assert!(tagged.child_source_identical);
}

#[test]
fn private_nested_error_preserves_logical_path_and_source_identity() {
    let mut bytes = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-consumer/golden/public-parent.bin"
    ));
    let child_offset = PublicParent::LAYOUT.fields()[1].offset();
    bytes.0[child_offset] = 2;
    let error = private_error_facts(&bytes.0);
    assert_eq!(error.kind, ErrorKind::InvalidBool);
    assert_eq!(error.child_schema, Some("DirectChild"));
    assert_eq!(error.segment, Some(ErrorPathSegment::Field("child")));
    assert!(error.child_source_identical);
}

#[test]
fn generic_promoted_descriptors_are_monomorphization_specific() {
    assert_eq!(GenericBytes::<'static, 3>::LAYOUT.fields()[0].size(), 3);
    assert_eq!(GenericBytes::<'static, 7>::LAYOUT.fields()[0].size(), 7);
    let three = generic_layout::<3>();
    let seven = generic_layout::<7>();
    assert_eq!((three.1, three.2), (3, 3));
    assert_eq!((seven.1, seven.2), (7, 7));
    assert!(seven.0 > three.0);
}

#[test]
fn nested_and_tagged_children_cross_the_real_crate_boundary() {
    assert_eq!(
        composition_parent_fixture().len(),
        CompositionParent::SCHEMA_SIZE
    );
    let (valid, nested, sentinel, number) = composition_from_fixture();
    assert_eq!(valid, 1);
    assert_eq!(nested, 0x5678);
    assert_eq!(sentinel, 0x65);
    assert_eq!(number, 0x0102_0304);
    assert_eq!(
        TrailingProjection::LAYOUT.fields().last().unwrap().name(),
        "sentinel"
    );
}

#[test]
fn exported_nonzero_children_compose_as_downstream_optionals_without_support_leaks() {
    assert_eq!(OptionalChild::SCHEMA_SIZE, 4);
    assert_eq!(
        optional_parent_none_from_zeroed(),
        zero_schema_cross_crate_consumer::OptionalObservation {
            code: None,
            child: None,
            codes: None,
        }
    );

    let bytes = Aligned([0; OptionalParent::SCHEMA_SIZE]);
    let view = OptionalParent::access(&bytes.0).expect("zero sentinel option fields are absent");
    let _: OptionalParent = view.copy_into();
    assert!(
        OptionalParent::LAYOUT
            .fields()
            .iter()
            .all(|field| field.is_optional())
    );

    assert_eq!(
        optional_parent_mutation_and_patch(),
        zero_schema_cross_crate_consumer::OptionalObservation {
            code: Some(OptionalCode::Two),
            child: Some((OptionalCode::One, 0x5678)),
            codes: Some([OptionalCode::Two, OptionalCode::One]),
        }
    );
}
