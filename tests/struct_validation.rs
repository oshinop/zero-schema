use std::cell::RefCell;
use zero_schema::{
    ErrorKind, ErrorPathSegment, SchemaError, ValidationContext, ValidationFailure,
    ValidationOperation, ZeroSchema,
};

thread_local! { static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; }
fn event(value: &'static str) {
    EVENTS.with(|events| events.borrow_mut().push(value));
}
fn take() -> Vec<&'static str> {
    EVENTS.with(|events| core::mem::take(&mut *events.borrow_mut()))
}
fn clear() {
    let _ = take();
}

fn context(context: &ValidationContext<'_>, field: Option<&str>, operation: ValidationOperation) {
    assert_eq!(context.layout().name(), "Validated");
    assert_eq!(context.field(), field);
    assert_eq!(context.variant(), None);
    assert_eq!(context.operation(), operation);
}
fn validate_first(value: &u8, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    context(c, Some("first"), c.operation());
    event(if c.operation() == ValidationOperation::Decode {
        "d:first"
    } else {
        "e:first"
    });
    if *value == 7 {
        Err(ValidationFailure::new(701, "first rejected"))
    } else {
        Ok(())
    }
}
fn validate_raw(value: &u16, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    context(c, Some("type"), c.operation());
    event(if c.operation() == ValidationOperation::Decode {
        "d:type"
    } else {
        "e:type"
    });
    if *value == 9 {
        Err(ValidationFailure::new(709, "raw rejected"))
    } else {
        Ok(())
    }
}
fn validate_last(value: &u32, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    context(c, Some("last"), c.operation());
    event(if c.operation() == ValidationOperation::Decode {
        "d:last"
    } else {
        "e:last"
    });
    if *value == 99 {
        Err(ValidationFailure::new(799, "last rejected"))
    } else {
        Ok(())
    }
}
fn validate_whole(value: &Validated, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    context(c, None, c.operation());
    event(if c.operation() == ValidationOperation::Decode {
        "d:whole"
    } else {
        "e:whole"
    });
    if value.last == 88 {
        Err(ValidationFailure::new(788, "whole rejected"))
    } else {
        Ok(())
    }
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding="zero", validate_with=validate_whole)]
struct Validated {
    #[zero(range=1..=10, must_equal=5, validate_with=validate_first)]
    first: u8,
    #[zero(validate_with=validate_raw)]
    r#type: u16,
    #[zero(validate_with=validate_last)]
    last: u32,
}
fn valid() -> Validated {
    Validated {
        first: 5,
        r#type: 3,
        last: 4,
    }
}
fn encode(
    value: &Validated,
) -> zero_schema::AlignedBytes<
    <Validated as zero_schema::ZeroSchemaType>::Wire,
    { Validated::WIRE_SIZE },
> {
    let mut b = zero_schema::make_buffer_for!(Validated);
    value.encode_into(b.as_bytes_mut()).unwrap();
    b
}
fn assert_error<E: SchemaError>(
    e: &E,
    kind: ErrorKind,
    segment: Option<ErrorPathSegment>,
    code: Option<u32>,
    text: &str,
) {
    assert_eq!(e.kind(), kind);
    assert_eq!(e.schema(), "Validated");
    assert_eq!(e.segment(), segment);
    assert_eq!(e.validation_code(), code);
    assert_eq!(e.to_string(), text);
}

#[test]
fn declaration_order_context_counts_and_whole_last() {
    clear();
    let b = encode(&valid());
    assert_eq!(take(), ["e:first", "e:type", "e:last", "e:whole"]);
    assert_eq!(Validated::parse(b.as_bytes()).unwrap(), valid());
    assert_eq!(take(), ["d:first", "d:type", "d:last", "d:whole"]);
}
#[test]
fn range_and_must_equal_precede_callback_and_preserve_destination() {
    clear();
    let mut b = zero_schema::make_buffer_for!(Validated);
    b.as_bytes_mut().fill(0xa5);
    let before = b.as_bytes().to_vec();
    let e = Validated {
        first: 11,
        r#type: 3,
        last: 4,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    assert_error(
        &e,
        ErrorKind::RangeViolation,
        Some(ErrorPathSegment::Field("first")),
        None,
        "Validated.first: value violates configured range",
    );
    assert!(take().is_empty());
    assert_eq!(b.as_bytes(), before);
    let e = Validated {
        first: 6,
        r#type: 3,
        last: 4,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    assert_error(
        &e,
        ErrorKind::MustEqualViolation,
        Some(ErrorPathSegment::Field("first")),
        None,
        "Validated.first: value differs from required constant",
    );
    assert!(take().is_empty());
    assert_eq!(b.as_bytes(), before);
}
#[test]
fn custom_field_and_whole_errors_are_structured_and_transactional() {
    clear();
    let mut b = zero_schema::make_buffer_for!(Validated);
    b.as_bytes_mut().fill(0xcc);
    let before = b.as_bytes().to_vec();
    let e = Validated {
        first: 5,
        r#type: 9,
        last: 4,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    assert_error(
        &e,
        ErrorKind::CustomValidation,
        Some(ErrorPathSegment::Field("type")),
        Some(709),
        "Validated.type: raw rejected (validation code 709)",
    );
    assert_eq!(take(), ["e:first", "e:type"]);
    assert_eq!(b.as_bytes(), before);
    let source = core::error::Error::source(&e)
        .unwrap()
        .downcast_ref::<ValidationFailure>()
        .unwrap();
    assert_eq!((source.code(), source.message()), (709, "raw rejected"));
    let e = Validated {
        first: 5,
        r#type: 3,
        last: 88,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap_err();
    assert_error(
        &e,
        ErrorKind::CustomValidation,
        None,
        Some(788),
        "Validated: whole rejected (validation code 788)",
    );
    assert_eq!(take(), ["e:first", "e:type", "e:last", "e:whole"]);
    assert_eq!(b.as_bytes(), before);
}
#[test]
fn padding_is_after_fields_before_whole() {
    clear();
    let mut b = encode(&valid());
    clear();
    let r = Validated::LAYOUT
        .padding()
        .iter()
        .find(|r| r.start() < r.end())
        .unwrap();
    let offset = r.start();
    b.as_bytes_mut()[offset] = 1;
    let e = Validated::parse(b.as_bytes()).unwrap_err();
    assert_error(
        &e,
        ErrorKind::NonZeroPadding,
        None,
        None,
        &format!("Validated: nonzero padding byte at offset {offset}"),
    );
    assert_eq!(take(), ["d:first", "d:type", "d:last"]);
}
#[test]
fn exact_and_prefix_consumption() {
    clear();
    let b = encode(&valid());
    clear();
    assert_eq!(Validated::parse(b.as_bytes()).unwrap(), valid());
    let mut bytes = vec![0; Validated::WIRE_SIZE + 3];
    bytes[..Validated::WIRE_SIZE].copy_from_slice(b.as_bytes());
    bytes[Validated::WIRE_SIZE..].copy_from_slice(&[0xde, 0xad, 0xbe]);
    let (value, rest) = Validated::parse_prefix(&bytes).unwrap();
    assert_eq!(value, valid());
    assert_eq!(rest, [0xde, 0xad, 0xbe]);
}

fn validate_text(value: &str, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    assert_eq!(c.layout().name(), "Borrowed");
    assert_eq!(c.field(), Some("text"));
    assert_eq!(c.variant(), None);
    event(if c.operation() == ValidationOperation::Decode {
        "d:borrowed"
    } else {
        "e:borrowed"
    });
    if value == "bad" {
        Err(ValidationFailure::new(733, "borrowed rejected"))
    } else {
        Ok(())
    }
}
#[derive(Debug, ZeroSchema)]
struct Borrowed<'a> {
    #[zero(capacity=4,len_type=u8,validate_with=validate_text)]
    text: &'a str,
}
#[test]
fn decode_borrowed_view_and_earlier_validator_precedence() {
    clear();
    let mut b = zero_schema::make_buffer_for!(Borrowed<'static>);
    Borrowed { text: "good" }
        .encode_into(b.as_bytes_mut())
        .unwrap();
    clear();
    let field = &Borrowed::LAYOUT.fields()[0];
    let string = match field.kind() {
        zero_schema::FieldKind::String(s) => s,
        _ => panic!(),
    };
    let start = field.offset() + string.data_offset();
    b.as_bytes_mut()[field.offset()] = 3;
    b.as_bytes_mut()[start..start + 3].copy_from_slice(b"bad");
    let e = Borrowed::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::CustomValidation);
    assert_eq!(e.segment(), Some(ErrorPathSegment::Field("text")));
    assert_eq!(e.validation_code(), Some(733));
    assert_eq!(
        e.to_string(),
        "Borrowed.text: borrowed rejected (validation code 733)"
    );
    assert_eq!(take(), ["d:borrowed"]);
}

fn validate_leading(value: &u8, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    assert_eq!(c.layout().name(), "DecodeOrder");
    assert_eq!(c.field(), Some("leading"));
    event("d:leading");
    if *value == 7 {
        Err(ValidationFailure::new(707, "leading rejected"))
    } else {
        Ok(())
    }
}
#[derive(Debug, ZeroSchema)]
struct DecodeOrder {
    #[zero(validate_with=validate_leading)]
    leading: u8,
    flag: bool,
}
#[test]
fn declaration_order_callback_failure_beats_later_malformed_field() {
    clear();
    let mut b = zero_schema::make_buffer_for!(DecodeOrder);
    DecodeOrder {
        leading: 1,
        flag: true,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap();
    clear();
    let leading = DecodeOrder::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "leading")
        .unwrap()
        .offset();
    let flag = DecodeOrder::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "flag")
        .unwrap()
        .offset();
    b.as_bytes_mut()[leading] = 7;
    b.as_bytes_mut()[flag] = 2;
    let e = DecodeOrder::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::CustomValidation);
    assert_eq!(e.segment(), Some(ErrorPathSegment::Field("leading")));
    assert_eq!(e.validation_code(), Some(707));
    assert_eq!(take(), ["d:leading"]);
}

#[test]
fn decode_range_and_must_equal_errors_are_structured_before_callbacks() {
    clear();
    let mut b = encode(&valid());
    clear();
    let offset = Validated::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "first")
        .unwrap()
        .offset();
    b.as_bytes_mut()[offset] = 11;
    let e = Validated::parse(b.as_bytes()).unwrap_err();
    assert_error(
        &e,
        ErrorKind::RangeViolation,
        Some(ErrorPathSegment::Field("first")),
        None,
        "Validated.first: value violates configured range",
    );
    assert!(take().is_empty());
    b.as_bytes_mut()[offset] = 6;
    let e = Validated::parse(b.as_bytes()).unwrap_err();
    assert_error(
        &e,
        ErrorKind::MustEqualViolation,
        Some(ErrorPathSegment::Field("first")),
        None,
        "Validated.first: value differs from required constant",
    );
    assert!(take().is_empty());
}
