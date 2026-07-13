use zero_schema::{ErrorKind, ErrorPathSegment, FieldKind, TypeKind, ZeroSchemaType};
use zero_schema_cross_crate_child::{
    BigCode, BorrowedChild, ChildMessage, DirectChild, GenericBytes, TaggedData, TrailingProjection,
};
use zero_schema_cross_crate_consumer::{
    PublicParent, PublicPrivateParent, child_message_layout, child_message_unknown_facts,
    composition_roundtrip, generic_layout, private_error_facts, roundtrip_borrowed,
    roundtrip_private, roundtrip_truly_private, truly_private_error_facts,
};

fn requires_only_schema<T: ZeroSchemaType>() {}

#[test]
fn derive_only_public_children_need_no_standard_traits() {
    requires_only_schema::<BigCode>();
    requires_only_schema::<DirectChild>();
    requires_only_schema::<BorrowedChild<'static>>();
    requires_only_schema::<ChildMessage>();
}

#[test]
fn private_child_roundtrips_through_public_parent_without_exposing_support() {
    let (bytes, observed) = roundtrip_private(7, true);
    assert_eq!(observed.prefix, 7);
    assert!(observed.valid);
    assert_eq!(bytes.len(), PublicParent::WIRE_SIZE);
    assert_eq!(PublicParent::LAYOUT.name(), "PublicParent");
    assert_eq!(PublicParent::LAYOUT.kind(), TypeKind::Struct);
    match PublicParent::LAYOUT.fields()[1].kind() {
        FieldKind::Schema { layout } => assert_eq!(layout.name(), "DirectChild"),
        other => panic!("unexpected child descriptor: {other:?}"),
    }
}

#[test]
fn genuinely_private_child_is_usable_behind_public_api() {
    let (bytes, observed) = roundtrip_truly_private(11, false, BigCode::r#type);
    assert_eq!(observed.prefix, 11);
    assert!(!observed.valid);
    assert_eq!(bytes.len(), PublicPrivateParent::WIRE_SIZE);
    let child = &PublicPrivateParent::LAYOUT.fields()[1];
    match child.kind() {
        FieldKind::Schema { layout } => assert_eq!(layout.name(), "PrivateChild"),
        other => panic!("unexpected private child descriptor: {other:?}"),
    }
    let mut malformed = bytes;
    malformed[child.offset()] = 2;
    let error = truly_private_error_facts(&malformed);
    assert_eq!(
        error.display,
        "PublicPrivateParent.child.valid: invalid boolean value 2; expected 0 or 1"
    );
    assert_eq!(error.kind, ErrorKind::InvalidBool);
    assert_eq!(error.segment, Some(ErrorPathSegment::Field("child")));
    assert_eq!(error.child_schema, Some("PrivateChild"));
    assert!(error.child_source_identical);
}

#[test]
fn scalar_and_tagged_metadata_and_diagnostics_survive_crate_boundaries() {
    assert_eq!(
        zero_schema_cross_crate_consumer::encode_big(BigCode::Ready),
        [0x01, 0x02]
    );
    assert_eq!(
        zero_schema_cross_crate_consumer::parse_big(&[0x01, 0x02]).unwrap(),
        0x0102
    );
    let scalar = zero_schema_cross_crate_consumer::big_unknown_facts(&[0, 0]);
    assert_eq!(scalar.kind, ErrorKind::UnknownEnumValue);
    assert_eq!(scalar.schema, "BigCode");
    assert_eq!(scalar.display, "BigCode: unknown enum value 0");
    let (size, align, variants, first, second) = child_message_layout();
    assert_eq!(
        (size, align, variants, first, second),
        (
            ChildMessage::WIRE_SIZE,
            ChildMessage::WIRE_ALIGN,
            2,
            "Empty",
            "Data"
        )
    );
    let mut bytes = vec![0; ChildMessage::WIRE_SIZE];
    bytes[0] = 0xff;
    let tagged = child_message_unknown_facts(&bytes);
    assert_eq!(tagged.kind, ErrorKind::UnknownUnionTag);
    assert_eq!(tagged.schema, "ChildMessage");
    assert_eq!(tagged.display, "ChildMessage: unknown union tag 255");
}

#[test]
fn private_nested_error_preserves_logical_path_and_source_identity() {
    let (mut bytes, _) = roundtrip_private(3, true);
    let child_offset = PublicParent::LAYOUT.fields()[1].offset();
    bytes[child_offset] = 2;
    let error = private_error_facts(&bytes);
    assert_eq!(
        error.display,
        "PublicParent.child.valid: invalid boolean value 2; expected 0 or 1"
    );
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
fn borrowed_child_is_live_and_points_into_the_consumer_buffer() {
    let (bytes, text_offset) = roundtrip_borrowed("embedded", &[4, 5]);
    assert!(text_offset < bytes.len());
    assert_eq!(&bytes[text_offset..text_offset + 8], b"embedded");
}

#[test]
fn nested_and_tagged_children_cross_the_real_crate_boundary() {
    let (bytes, valid, nested, sentinel) = composition_roundtrip(ChildMessage::Data(TaggedData {
        number: 0x0102_0304,
    }));
    assert!(!bytes.is_empty());
    assert_eq!(valid, 1);
    assert_eq!(nested, 0x5678);
    assert_eq!(sentinel, 0xa5);
    assert_eq!(
        TrailingProjection::LAYOUT.fields().last().unwrap().name(),
        "sentinel"
    );
}
