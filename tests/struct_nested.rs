use zero_schema::ZeroSchema;

#[derive(ZeroSchema)]
struct Child {
    valid: bool,
    value: u32,
}

#[derive(ZeroSchema)]
struct Parent {
    prefix: u8,
    child: Child,
}

#[derive(ZeroSchema)]
struct LateParent<'a> {
    child: LateChild<'a>,
}

#[derive(ZeroSchema)]
struct LateChild<'a> {
    #[zero(capacity = 2)]
    text: &'a str,
}
#[test]
fn nested_nonzero_roundtrip_and_metadata() {
    let value = Parent {
        prefix: 7,
        child: Child {
            valid: true,
            value: 0x1122_3344,
        },
    };
    let mut buffer = zero_schema::make_buffer_for!(Parent);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded = Parent::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded.prefix, 7);
    assert!(decoded.child.valid);
    assert_eq!(decoded.child.value, 0x1122_3344);
    let field = &Parent::LAYOUT.fields()[1];
    match field.kind() {
        zero_schema::FieldKind::Schema { layout } => assert_eq!(layout.name(), "Child"),
        other => panic!("unexpected nested field metadata: {other:?}"),
    }
}

#[test]
fn nested_errors_delegate_and_encode_is_transactional() {
    let mut valid = zero_schema::make_buffer_for!(Parent);
    Parent {
        prefix: 1,
        child: Child {
            valid: true,
            value: 9,
        },
    }
    .encode_into(valid.as_bytes_mut())
    .unwrap();
    let child_offset = Parent::LAYOUT.fields()[1].offset();
    valid.as_bytes_mut()[child_offset] = 2;
    let error = match Parent::parse(valid.as_bytes()) {
        Err(error) => error,
        Ok(_) => panic!("invalid child bool decoded"),
    };
    assert_eq!(
        error.to_string(),
        "Parent.child.valid: invalid boolean value 2; expected 0 or 1"
    );
    let child = zero_schema::SchemaError::child(&error).unwrap();
    let source = std::error::Error::source(&error).unwrap();
    assert!(core::ptr::eq(
        child as *const dyn zero_schema::SchemaError as *const (),
        source as *const dyn std::error::Error as *const ()
    ));
    assert!(source.downcast_ref::<ChildDecodeError>().is_some());

    let value = LateParent {
        child: LateChild { text: "too long" },
    };
    let mut destination = zero_schema::make_buffer_for!(LateParent);
    destination.as_bytes_mut().fill(0xa5);
    let before = destination.as_bytes().to_vec();
    let error = value.encode_into(destination.as_bytes_mut()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "LateParent.child.text: length 8 exceeds encoding capacity 2"
    );
    assert_eq!(destination.as_bytes(), before);
}
