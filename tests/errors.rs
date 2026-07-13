use core::error::Error as _;
use core::ffi::CStr;
use zero_schema::{
    ErrorKind, ErrorPathSegment, LayoutError, SchemaError, ValidationContext, ValidationFailure,
    ZeroSchema,
};

#[derive(Debug, ZeroSchema)]
#[repr(u8)]
enum Code {
    Good = 1,
}

fn reject_9(value: &u8, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    if *value == 9 {
        Err(ValidationFailure::new(909, "nine rejected"))
    } else {
        Ok(())
    }
}

#[derive(Debug, ZeroSchema)]
#[zero(padding = "zero")]
struct Direct<'a> {
    flag: bool,
    #[zero(capacity = 2, len_type = u8, tail = "zero")]
    text: &'a str,
    #[zero(capacity = 3)]
    c: &'a CStr,
    #[zero(range = 1..=3)]
    ranged: u8,
    #[zero(must_equal = 7)]
    fixed: u8,
    #[zero(validate_with = reject_9)]
    custom: u8,
}

#[derive(Debug, ZeroSchema)]
struct Leaf {
    child: bool,
}
#[derive(Debug, ZeroSchema)]
#[repr(u8)]
enum ChoiceTag {
    Item = 1,
}
#[derive(Debug, ZeroSchema)]
#[zero(tag = ChoiceTag)]
enum Choice<T>
where
    T: zero_schema::ZeroSchemaType,
{
    #[zero(tag = ChoiceTag::Item)]
    Item(T),
}
#[derive(Debug, ZeroSchema)]
struct Parent<T>
where
    T: zero_schema::ZeroSchemaType,
{
    field: Choice<T>,
}

#[derive(Debug, ZeroSchema)]
#[repr(u8)]
enum ExternalTag {
    Item = 1,
    Spare = 2,
}
#[derive(Debug, ZeroSchema)]
#[zero(tag = ExternalTag)]
enum ExternalMessage {
    #[zero(tag = ExternalTag::Item)]
    Item(Leaf),
}
#[derive(Debug, ZeroSchema)]
struct Envelope {
    tag: ExternalTag,
    #[zero(tag_field = tag)]
    payload: ExternalMessage,
}

fn inspect<E: SchemaError>(
    error: &E,
    kind: ErrorKind,
    schema: &'static str,
    segment: Option<ErrorPathSegment>,
    code: Option<u32>,
    display: &str,
) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.schema(), schema);
    assert_eq!(error.segment(), segment);
    assert_eq!(error.validation_code(), code);
    assert_eq!(error.to_string(), display);
}

#[test]
fn scalar_and_layout_errors_are_structured_and_downcastable() {
    let mut bytes = zero_schema::make_buffer_for!(Code);
    bytes.as_bytes_mut()[0] = 8;
    let error = Code::parse(bytes.as_bytes()).unwrap_err();
    inspect(
        &error,
        ErrorKind::UnknownEnumValue,
        "Code",
        None,
        None,
        "Code: unknown enum value 8",
    );
    assert!(error.child().is_none());
    assert!(error.source().is_none());

    let error = Code::parse(&[]).unwrap_err();
    inspect(
        &error,
        ErrorKind::Layout,
        "Code",
        None,
        None,
        "Code: incorrect size: expected 1 bytes, got 0",
    );
    assert_eq!(
        error.source().unwrap().downcast_ref::<LayoutError>(),
        Some(&LayoutError::IncorrectSize {
            expected: 1,
            actual: 0
        })
    );
}

fn valid_direct<'a>() -> Direct<'a> {
    Direct {
        flag: true,
        text: "a",
        c: c"x",
        ranged: 2,
        fixed: 7,
        custom: 1,
    }
}
fn encoded_direct() -> zero_schema::AlignedBytes<
    <Direct<'static> as zero_schema::ZeroSchemaType>::Wire,
    { Direct::WIRE_SIZE },
> {
    let mut b = zero_schema::make_buffer_for!(Direct);
    valid_direct().encode_into(b.as_bytes_mut()).unwrap();
    b
}

#[test]
fn direct_decode_variants_report_exact_leaf_and_sources() {
    let field = |name| {
        Direct::LAYOUT
            .fields()
            .iter()
            .find(|f| f.name() == name)
            .unwrap()
            .offset()
    };
    let mut b = encoded_direct();
    b.as_bytes_mut()[field("flag")] = 2;
    let e = Direct::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::InvalidBool,
        "Direct",
        Some(ErrorPathSegment::Field("flag")),
        None,
        "Direct.flag: invalid boolean value 2; expected 0 or 1",
    );

    let mut b = encoded_direct();
    b.as_bytes_mut()[field("text")] = 3;
    let e = Direct::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::LengthOutOfBounds,
        "Direct",
        Some(ErrorPathSegment::Field("text")),
        None,
        "Direct.text: length 3 exceeds capacity 2",
    );

    let mut b = encoded_direct();
    let o = field("text");
    b.as_bytes_mut()[o] = 1;
    b.as_bytes_mut()[o + 1] = 0xff;
    let e = Direct::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::InvalidUtf8,
        "Direct",
        Some(ErrorPathSegment::Field("text")),
        None,
        "Direct.text: invalid UTF-8: invalid utf-8 sequence of 1 bytes from index 0",
    );
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<core::str::Utf8Error>()
            .is_some()
    );

    let mut b = encoded_direct();
    let o = field("c");
    b.as_bytes_mut()[o..o + 3].fill(b'x');
    let e = Direct::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::MissingNul,
        "Direct",
        Some(ErrorPathSegment::Field("c")),
        None,
        "Direct.c: missing NUL terminator",
    );

    let mut b = encoded_direct();
    let o = field("text");
    b.as_bytes_mut()[o + 2] = 1;
    let e = Direct::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::NonZeroTail,
        "Direct",
        Some(ErrorPathSegment::Field("text")),
        None,
        "Direct.text: nonzero tail at logical offset 1",
    );
}

#[test]
fn direct_encode_semantic_variants_and_validation_source() {
    let mut b = zero_schema::make_buffer_for!(Direct);
    let e = Direct {
        text: "abc",
        ..valid_direct()
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    inspect(
        &e,
        ErrorKind::CapacityExceeded,
        "Direct",
        Some(ErrorPathSegment::Field("text")),
        None,
        "Direct.text: length 3 exceeds encoding capacity 2",
    );
    let e = Direct {
        ranged: 4,
        ..valid_direct()
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    inspect(
        &e,
        ErrorKind::RangeViolation,
        "Direct",
        Some(ErrorPathSegment::Field("ranged")),
        None,
        "Direct.ranged: value violates configured range",
    );
    let e = Direct {
        fixed: 8,
        ..valid_direct()
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    inspect(
        &e,
        ErrorKind::MustEqualViolation,
        "Direct",
        Some(ErrorPathSegment::Field("fixed")),
        None,
        "Direct.fixed: value differs from required constant",
    );
    let e = Direct {
        custom: 9,
        ..valid_direct()
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    inspect(
        &e,
        ErrorKind::CustomValidation,
        "Direct",
        Some(ErrorPathSegment::Field("custom")),
        Some(909),
        "Direct.custom: nine rejected (validation code 909)",
    );
    assert_eq!(
        e.source()
            .unwrap()
            .downcast_ref::<ValidationFailure>()
            .map(|v| (v.code(), v.message())),
        Some((909, "nine rejected"))
    );
}

#[test]
fn tagged_direct_and_generic_nested_path_have_no_wrapper_node() {
    let mut bad = zero_schema::make_buffer_for!(Choice<Leaf>);
    bad.as_bytes_mut()[0] = 99;
    let e = Choice::<Leaf>::parse(bad.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::UnknownUnionTag,
        "Choice",
        None,
        None,
        "Choice: unknown union tag 99",
    );

    let value = Parent {
        field: Choice::Item(Leaf { child: true }),
    };
    let mut b = zero_schema::make_buffer_for!(Parent<Leaf>);
    value.encode_into(b.as_bytes_mut()).unwrap();
    let choice_offset = Parent::<Leaf>::LAYOUT.fields()[0].offset();
    let payload_offset = match Choice::<Leaf>::LAYOUT.kind() {
        zero_schema::TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => unreachable!(),
    };
    b.as_bytes_mut()[choice_offset + payload_offset] = 2;
    let e = Parent::<Leaf>::parse(b.as_bytes()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "Parent.field.Item.child: invalid boolean value 2; expected 0 or 1"
    );
    assert_eq!(
        (e.schema(), e.segment()),
        ("Parent", Some(ErrorPathSegment::Field("field")))
    );
    let tagged = e.child().unwrap();
    assert_eq!(
        (tagged.schema(), tagged.segment()),
        ("Choice", Some(ErrorPathSegment::Variant("Item")))
    );
    let leaf = tagged.child().unwrap();
    assert_eq!(
        (leaf.schema(), leaf.segment()),
        ("Leaf", Some(ErrorPathSegment::Field("child")))
    );
    assert!(leaf.child().is_none());
    let source = e.source().unwrap();
    assert!(core::ptr::eq(
        tagged as *const dyn SchemaError as *const (),
        source as *const dyn core::error::Error as *const ()
    ));
    assert_eq!(e.to_string().matches("Parent").count(), 1);
}

#[test]
fn external_known_unmapped_tag_is_nested_under_payload() {
    let mut b = zero_schema::make_buffer_for!(Envelope);
    let tag_offset = Envelope::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "tag")
        .unwrap()
        .offset();
    b.as_bytes_mut()[tag_offset] = 2;
    let e = Envelope::parse(b.as_bytes()).unwrap_err();
    inspect(
        &e,
        ErrorKind::UnknownUnionTag,
        "Envelope",
        Some(ErrorPathSegment::Field("payload")),
        None,
        "Envelope.payload: unknown union tag 2",
    );
    let child = e.child().unwrap();
    assert_eq!(
        (child.schema(), child.segment(), child.kind()),
        ("ExternalMessage", None, ErrorKind::UnknownUnionTag)
    );
    let source = e.source().unwrap();
    assert!(core::ptr::eq(
        child as *const dyn SchemaError as *const (),
        source as *const dyn core::error::Error as *const ()
    ));
}
