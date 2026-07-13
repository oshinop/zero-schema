use zero_schema::{ErrorKind, SchemaError, TypeKind, ZeroSchema, ZeroSchemaType};

#[derive(ZeroSchema)]
#[repr(u8)]
enum PlainTag {
    Empty = 1,
    Number = 2,
    Spare = 3,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Number {
    value: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = PlainTag, tail = "zero")]
enum Message {
    #[zero(tag = PlainTag::Empty)]
    Empty,
    #[zero(tag = PlainTag::Number)]
    Number(Number),
}

#[derive(ZeroSchema)]
#[repr(u8)]
enum UnitTag {
    First = 7,
    Second = 9,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = UnitTag)]
enum Units {
    #[zero(tag = UnitTag::First)]
    First,
    #[zero(tag = UnitTag::Second)]
    Second,
}

fn assert_no_standard_bounds<T: ZeroSchemaType>() {}

#[test]
fn internal_unit_and_newtype_round_trip_and_prefix() {
    assert_no_standard_bounds::<PlainTag>();
    assert_no_standard_bounds::<UnitTag>();

    let mut empty = zero_schema::make_buffer_for!(Message);
    Message::Empty.encode_into(empty.as_bytes_mut()).unwrap();
    assert_eq!(Message::parse(empty.as_bytes()).unwrap(), Message::Empty);

    let original = Message::Number(Number { value: 0x0102_0304 });
    let number = original.encode().unwrap();
    assert_eq!(Message::parse(number.as_bytes()).unwrap(), original);
    assert_eq!(original.encoded_len(), Message::WIRE_SIZE);

    let mut prefixed = vec![0u8; Message::WIRE_SIZE + 3];
    prefixed[..Message::WIRE_SIZE].copy_from_slice(number.as_bytes());
    prefixed[Message::WIRE_SIZE..].copy_from_slice(&[5, 6, 7]);
    let (parsed, rest) = Message::parse_prefix(&prefixed).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(rest, &[5, 6, 7]);
}

#[test]
fn all_unit_layout_round_trips() {
    let mut buffer = zero_schema::make_buffer_for!(Units);
    Units::Second.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(Units::parse(buffer.as_bytes()).unwrap(), Units::Second);
    assert_eq!(buffer.as_bytes()[0], 9);
    assert_eq!(Units::WIRE_SIZE, 1);
    match Units::LAYOUT.kind() {
        TypeKind::TaggedUnion {
            tag_offset,
            payload_size,
            payload_align,
            ..
        } => {
            assert_eq!(tag_offset, 0);
            assert_eq!(payload_size, 0);
            assert_eq!(payload_align, 1);
        }
        _ => panic!("expected tagged union layout"),
    }
}

#[test]
fn raw_unknown_known_unmapped_and_inactive_tail_are_exact() {
    let mut unknown = zero_schema::make_buffer_for!(Message);
    unknown.as_bytes_mut()[0] = 99;
    let error = Message::parse(unknown.as_bytes()).unwrap_err();
    assert!(matches!(
        error,
        MessageDecodeError::UnknownUnionTag { value: 99 }
    ));
    assert_eq!(error.kind(), ErrorKind::UnknownUnionTag);
    assert_eq!(error.schema(), "Message");
    assert_eq!(error.to_string(), "Message: unknown union tag 99");

    let mut unmapped = zero_schema::make_buffer_for!(Message);
    unmapped.as_bytes_mut()[0] = 3;
    let error = Message::parse(unmapped.as_bytes()).unwrap_err();
    assert!(matches!(
        error,
        MessageDecodeError::UnknownUnionTag { value: 3 }
    ));

    let mut inactive = zero_schema::make_buffer_for!(Message);
    Message::Empty.encode_into(inactive.as_bytes_mut()).unwrap();
    let payload_offset = match Message::LAYOUT.kind() {
        TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => panic!("expected tagged union layout"),
    };
    inactive.as_bytes_mut()[payload_offset + 1] = 1;
    let error = Message::parse(inactive.as_bytes()).unwrap_err();
    assert!(matches!(
        error,
        MessageDecodeError::NonZeroTail {
            variant: "Empty",
            offset: 1
        }
    ));
    assert_eq!(error.kind(), ErrorKind::NonZeroTail);
    assert_eq!(
        error.to_string(),
        "Message: nonzero inactive payload byte at offset 1"
    );
}

#[test]
fn internal_layout_alignment_and_misaligned_encode_are_enforced_transactionally() {
    let kind = Message::LAYOUT.kind();
    let (payload_offset, payload_size, payload_align) = match kind {
        TypeKind::TaggedUnion {
            tag_offset,
            payload_offset,
            payload_size,
            payload_align,
            ..
        } => {
            assert_eq!(tag_offset, 0);
            (payload_offset, payload_size, payload_align)
        }
        _ => panic!("expected tagged union layout"),
    };
    assert_eq!(payload_size, Number::WIRE_SIZE);
    assert_eq!(payload_align, Number::WIRE_ALIGN);
    assert_eq!(payload_offset % payload_align, 0);
    assert_eq!(Message::WIRE_ALIGN, Number::WIRE_ALIGN);

    let original = Message::Number(Number { value: 7 });
    let mut storage = [0xa5; Message::WIRE_SIZE + Message::WIRE_ALIGN];
    let base = storage.as_ptr() as usize;
    let offset = (0..Message::WIRE_ALIGN)
        .find(|offset| (base + offset) % Message::WIRE_ALIGN != 0)
        .unwrap();
    let destination = &mut storage[offset..offset + Message::WIRE_SIZE];
    let before = destination.to_vec();
    let error = original.encode_into(destination).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Layout);
    assert_eq!(destination, before);

    let mut wrong_size = vec![0xa5; Message::WIRE_SIZE - 1];
    let before = wrong_size.clone();
    let error = original.encode_into(&mut wrong_size).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Layout);
    assert_eq!(wrong_size, before);
}
