use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
struct Blob<'a, const N: usize> {
    bytes: &'a [u8; N],
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(borrow = 'a)]
struct Lifetimes<'a: 'b, 'b> {
    #[zero(capacity = 8)]
    text: &'b str,
    marker: &'a [u8; 1],
}

#[derive(ZeroSchema)]
struct PlainMarker {
    valid: bool,
}

#[derive(ZeroSchema)]
struct GenericChild<T> {
    value: T,
}

#[derive(ZeroSchema)]
struct GenericParent<T> {
    child: GenericChild<T>,
}

#[test]
fn const_generic_fixed_bytes_roundtrip_and_metadata() {
    let value = Blob::<3> { bytes: &[1, 2, 3] };
    let mut storage = [0u8; Blob::<3>::WIRE_SIZE + Blob::<3>::WIRE_ALIGN];
    let offset = storage.as_ptr().align_offset(Blob::<3>::WIRE_ALIGN);
    let bytes = &mut storage[offset..offset + Blob::<3>::WIRE_SIZE];
    value.encode_into(bytes).unwrap();
    assert_eq!(Blob::<3>::parse(bytes).unwrap(), value);
    assert_eq!(Blob::<3>::LAYOUT.fields()[0].size(), 3);
    assert_eq!(Blob::<5>::LAYOUT.fields()[0].size(), 5);
}

#[test]
fn lifetime_only_schema_keeps_buffer_and_outlives_decode() {
    let value = Lifetimes {
        text: "hello",
        marker: &[9],
    };
    let mut buffer = zero_schema::make_buffer_for!(Lifetimes<'static, 'static>);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded: Lifetimes<'_, '_> = Lifetimes::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(
        core::mem::size_of_val(&buffer),
        Lifetimes::<'static, 'static>::WIRE_STRIDE
    );
    assert_eq!(
        core::mem::align_of_val(&buffer),
        Lifetimes::<'static, 'static>::WIRE_ALIGN
    );
}

#[test]
fn nested_generic_errors_do_not_require_value_traits() {
    let value = GenericParent {
        child: GenericChild {
            value: PlainMarker { valid: true },
        },
    };
    let mut storage =
        [0u8; GenericParent::<PlainMarker>::WIRE_SIZE + GenericParent::<PlainMarker>::WIRE_ALIGN];
    let offset = storage
        .as_ptr()
        .align_offset(GenericParent::<PlainMarker>::WIRE_ALIGN);
    let bytes = &mut storage[offset..offset + GenericParent::<PlainMarker>::WIRE_SIZE];
    value.encode_into(bytes).unwrap();
    bytes[0] = 2;

    let error = match GenericParent::<PlainMarker>::parse(bytes) {
        Err(error) => error,
        Ok(_) => panic!("invalid bool decoded"),
    };
    assert_eq!(
        error.to_string(),
        "GenericParent.child.value.valid: invalid boolean value 2; expected 0 or 1"
    );
    let child = zero_schema::SchemaError::child(&error).unwrap();
    assert_eq!(child.schema(), "GenericChild");
    let source = std::error::Error::source(&error).unwrap();
    assert!(source.source().is_some());
    let _ = format!("{error:?}");
}
