use zero_schema::{FieldKind, SchemaError, TaggedUnion, TypeKind, ZeroSchema};

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Tag {
    Empty = 1,
    Number = 2,
    Spare = 3,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Number {
    value: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag, tail = "zero")]
enum Payload {
    #[zero(tag = Tag::Empty)]
    Empty,
    #[zero(tag = Tag::Number)]
    Number(Number),
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Envelope {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Payload,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct EnvelopeAfter {
    #[zero(tag_field = tag)]
    payload: Payload,
    tag: Tag,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Pair {
    tag: Tag,
    #[zero(tag_field = tag)]
    first: Payload,
    #[zero(tag_field = tag)]
    second: Payload,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum UTag {
    A = 7,
    B = 9,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = UTag)]
enum Units {
    #[zero(tag = UTag::A)]
    A,
    #[zero(tag = UTag::B)]
    B,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct UnitEnvelope {
    tag: UTag,
    #[zero(tag_field = tag)]
    payload: Units,
}

#[test]
fn external_storage_roundtrips_in_both_orders() {
    assert_eq!(
        Envelope::WIRE_SIZE,
        1 + core::mem::size_of::<<Payload as TaggedUnion>::PayloadWire>() + 3
    );
    let value = Envelope {
        tag: Tag::Number,
        payload: Payload::Number(Number { value: 0x1122_3344 }),
    };
    let mut buffer = zero_schema::make_buffer_for!(Envelope);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(buffer.as_bytes()[0], 2);
    assert_eq!(Envelope::parse(buffer.as_bytes()).unwrap(), value);
    let after = EnvelopeAfter {
        payload: Payload::Empty,
        tag: Tag::Empty,
    };
    let mut after_buffer = zero_schema::make_buffer_for!(EnvelopeAfter);
    after.encode_into(after_buffer.as_bytes_mut()).unwrap();
    assert_eq!(
        EnvelopeAfter::parse(after_buffer.as_bytes()).unwrap(),
        after
    );
    match Envelope::LAYOUT.fields()[1].kind() {
        FieldKind::ExternalTaggedUnion { tag_field, .. } => assert_eq!(tag_field, "tag"),
        _ => panic!("expected external union field"),
    }
}

#[test]
fn one_cached_tag_feeds_two_payloads() {
    let value = Pair {
        tag: Tag::Empty,
        first: Payload::Empty,
        second: Payload::Empty,
    };
    let mut buffer = zero_schema::make_buffer_for!(Pair);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(Pair::parse(buffer.as_bytes()).unwrap(), value);
}

#[test]
fn known_unmapped_is_payload_error_and_mismatch_is_transactional() {
    let mut buffer = zero_schema::make_buffer_for!(Envelope);
    buffer.as_bytes_mut()[0] = 3;
    let error = Envelope::parse(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.to_string(), "Envelope.payload: unknown union tag 3");
    assert_eq!(
        error.segment(),
        Some(zero_schema::ErrorPathSegment::Field("payload"))
    );

    let value = Envelope {
        tag: Tag::Empty,
        payload: Payload::Number(Number { value: 7 }),
    };
    let mut destination = zero_schema::make_buffer_for!(Envelope);
    destination.as_bytes_mut().fill(0xa5);
    let before = destination.as_bytes().to_vec();
    let error = value.encode_into(destination.as_bytes_mut()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Envelope.payload: external tag 1 does not match selected tag 2"
    );
    assert_eq!(destination.as_bytes(), before);
}

#[test]
fn all_unit_payload_storage_is_zero_sized() {
    assert_eq!(
        core::mem::size_of::<<Units as TaggedUnion>::PayloadWire>(),
        0
    );
    assert_eq!(UnitEnvelope::WIRE_SIZE, 1);
    let mut bytes = [0u8; 4];
    bytes[0] = 9;
    let (value, rest) = UnitEnvelope::parse_prefix(&bytes).unwrap();
    assert_eq!(
        value,
        UnitEnvelope {
            tag: UTag::B,
            payload: Units::B
        }
    );
    assert_eq!(rest, &[0, 0, 0]);
    assert!(matches!(
        Units::LAYOUT.kind(),
        TypeKind::TaggedUnion {
            payload_size: 0,
            ..
        }
    ));
}
