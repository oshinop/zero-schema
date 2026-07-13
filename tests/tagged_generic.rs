use zero_schema::{SchemaError, TypeKind, ZeroSchema};

#[derive(ZeroSchema)]
#[repr(u8)]
enum BorrowTag {
    Empty = 3,
    Text = 4,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Borrowed<'a> {
    #[zero(capacity = 8)]
    text: &'a str,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = BorrowTag, tail = "zero")]
enum BorrowedMessage<'a> {
    #[zero(tag = BorrowTag::Empty)]
    Empty,
    #[zero(tag = BorrowTag::Text)]
    Text(Borrowed<'a>),
}

#[derive(ZeroSchema)]
#[repr(u8)]
enum GenericTag {
    Empty = 1,
    Value = 2,
}

#[derive(ZeroSchema)]
struct GenericPayload<T> {
    value: T,
}

#[derive(ZeroSchema)]
struct Flag {
    valid: bool,
}

#[derive(ZeroSchema)]
#[zero(tag = GenericTag)]
enum GenericMessage<T> {
    #[zero(tag = GenericTag::Empty)]
    Empty,
    #[zero(tag = GenericTag::Value)]
    Value(GenericPayload<T>),
}

#[test]
fn type_generic_roundtrip_projected_error_and_concrete_storage_recipe() {
    let value = GenericMessage::Value(GenericPayload {
        value: Flag { valid: true },
    });
    let mut storage = zero_schema::make_buffer_for!(GenericMessage<Flag>);
    value.encode_into(storage.as_bytes_mut()).unwrap();
    assert!(matches!(
        GenericMessage::<Flag>::parse(storage.as_bytes()).unwrap(),
        GenericMessage::Value(_)
    ));
    let variant = GenericMessage::<Flag>::LAYOUT
        .variants()
        .iter()
        .find(|v| v.name() == "Value")
        .unwrap();
    assert_eq!(variant.payload().unwrap(), GenericPayload::<Flag>::LAYOUT);
    let payload_offset = match GenericMessage::<Flag>::LAYOUT.kind() {
        TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => unreachable!(),
    };
    storage.as_bytes_mut()[payload_offset] = 2;
    let error = match GenericMessage::<Flag>::parse(storage.as_bytes()) {
        Err(error) => error,
        Ok(_) => panic!("invalid bool decoded"),
    };
    assert_eq!(
        error.to_string(),
        "GenericMessage.Value.value.valid: invalid boolean value 2; expected 0 or 1"
    );
    assert_eq!(error.child().unwrap().schema(), "GenericPayload");
}

#[test]
fn lifetime_payload_uses_live_layout_and_erased_buffer() {
    let value = BorrowedMessage::Text(Borrowed { text: "hello" });
    let mut buffer = zero_schema::make_buffer_for!(BorrowedMessage<'static>);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded: BorrowedMessage<'_> = BorrowedMessage::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded, value);

    let (payload_offset, payload_align) = match BorrowedMessage::<'static>::LAYOUT.kind() {
        TypeKind::TaggedUnion {
            payload_offset,
            payload_align,
            ..
        } => (payload_offset, payload_align),
        _ => panic!("expected tagged union"),
    };
    assert_eq!(payload_align, Borrowed::<'static>::WIRE_ALIGN);
    assert_eq!(payload_offset % payload_align, 0);
    assert_eq!(
        core::mem::align_of_val(&buffer),
        BorrowedMessage::<'static>::WIRE_ALIGN
    );
    assert_eq!(
        core::mem::size_of_val(&buffer),
        BorrowedMessage::<'static>::WIRE_STRIDE
    );
}

#[test]
fn lifetime_payload_preserves_nested_error_projection() {
    let value = BorrowedMessage::Text(Borrowed { text: "ok" });
    let mut buffer = zero_schema::make_buffer_for!(BorrowedMessage<'static>);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let payload_offset = match BorrowedMessage::<'static>::LAYOUT.kind() {
        TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => unreachable!(),
    };
    buffer.as_bytes_mut()[payload_offset] = 9;
    let error = BorrowedMessage::parse(buffer.as_bytes()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "BorrowedMessage.Text.text: length 9 exceeds capacity 8"
    );
    assert_eq!(error.child().unwrap().schema(), "Borrowed");
}
