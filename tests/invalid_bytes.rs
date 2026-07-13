use core::error::Error as _;
use core::ffi::CStr;
use widestring::{U16CStr, U16Str};
use zero_schema::{
    ErrorKind, ErrorPathSegment, FieldKind, SchemaError, TypeKind, ValidationContext,
    ValidationFailure, ZeroSchema,
};

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Choice {
    One = 1,
    Two = 2,
}

fn reject_seven(value: &u8, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    if *value == 7 {
        Err(ValidationFailure::new(707, "seven rejected"))
    } else {
        Ok(())
    }
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(padding = "zero")]
struct Malformed<'a> {
    flag: bool,
    choice: Choice,
    #[zero(capacity = 3, len_type = u8, tail = "zero")]
    text: &'a str,
    #[zero(capacity = 3, tail = "zero")]
    c: &'a CStr,
    #[zero(capacity = 2, len_type = u8, endian = "native", tail = "zero")]
    wide: &'a U16Str,
    #[zero(capacity = 2, endian = "native", tail = "zero")]
    wide_c: &'a U16CStr,
    #[zero(range = 1..=9, must_equal = 5, validate_with = reject_seven)]
    checked: u8,
}

fn valid() -> Malformed<'static> {
    Malformed {
        flag: true,
        choice: Choice::One,
        text: "ab",
        c: c"a",
        wide: U16Str::from_slice(&[0x41]),
        wide_c: U16CStr::from_slice(&[0x42, 0]).unwrap(),
        checked: 5,
    }
}
fn encoded() -> zero_schema::AlignedBytes<
    <Malformed<'static> as zero_schema::ZeroSchemaType>::Wire,
    { Malformed::WIRE_SIZE },
> {
    let mut b = zero_schema::make_buffer_for!(Malformed);
    valid().encode_into(b.as_bytes_mut()).unwrap();
    b
}
fn field(name: &str) -> zero_schema::FieldDescriptor {
    *Malformed::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == name)
        .unwrap()
}
fn string(name: &str) -> (usize, zero_schema::StringDescriptor) {
    let f = field(name);
    match f.kind() {
        FieldKind::String(s) => (f.offset(), s),
        _ => panic!(),
    }
}
fn assert_leaf<E: SchemaError>(e: &E, kind: ErrorKind, field: Option<&'static str>) {
    assert_eq!(e.kind(), kind);
    assert_eq!(e.schema(), "Malformed");
    assert_eq!(e.segment(), field.map(ErrorPathSegment::Field));
}

#[test]
fn bool_enum_and_capacity_boundaries_are_exact() {
    let mut b = encoded();
    b.as_bytes_mut()[field("flag").offset()] = 2;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::InvalidBool, Some("flag"));
    assert_eq!(
        e.to_string(),
        "Malformed.flag: invalid boolean value 2; expected 0 or 1"
    );
    assert!(e.source().is_none());
    let mut b = encoded();
    b.as_bytes_mut()[field("choice").offset()] = 9;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::UnknownEnumValue, Some("choice"));
    assert_eq!(e.to_string(), "Malformed.choice: unknown enum value 9");
    let (o, s) = string("text");
    let mut b = encoded();
    b.as_bytes_mut()[o + s.length().unwrap().offset()] = 3;
    assert_eq!(Malformed::parse(b.as_bytes()).unwrap().text, "ab\0");
    b.as_bytes_mut()[o + s.length().unwrap().offset()] = 4;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::LengthOutOfBounds, Some("text"));
    assert_eq!(e.to_string(), "Malformed.text: length 4 exceeds capacity 3");
    let (o, s) = string("wide");
    let mut b = encoded();
    b.as_bytes_mut()[o + s.length().unwrap().offset()] = 2;
    assert_eq!(
        Malformed::parse(b.as_bytes()).unwrap().wide.as_slice(),
        &[0x41, 0]
    );
    b.as_bytes_mut()[o + s.length().unwrap().offset()] = 3;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::LengthOutOfBounds, Some("wide"));
    assert_eq!(e.to_string(), "Malformed.wide: length 3 exceeds capacity 2");
}

#[test]
fn utf8_nul_and_tail_offsets_use_documented_units() {
    let (o, s) = string("text");
    let mut b = encoded();
    b.as_bytes_mut()[o + s.data_offset()] = 0xff;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::InvalidUtf8, Some("text"));
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<core::str::Utf8Error>()
            .is_some()
    );
    let mut b = encoded();
    b.as_bytes_mut()[o + s.data_offset() + 2] = 1;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::NonZeroTail, Some("text"));
    assert_eq!(
        e.to_string(),
        "Malformed.text: nonzero tail at logical offset 2"
    );
    let (o, s) = string("c");
    let mut b = encoded();
    b.as_bytes_mut()[o..o + s.capacity()].fill(b'x');
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::MissingNul, Some("c"));
    let mut b = encoded();
    b.as_bytes_mut()[o] = 0;
    b.as_bytes_mut()[o + 1] = 9;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::NonZeroTail, Some("c"));
    assert_eq!(
        e.to_string(),
        "Malformed.c: nonzero tail at logical offset 1"
    );
    let (o, s) = string("wide_c");
    let mut b = encoded();
    for unit in b.as_bytes_mut()[o..o + s.capacity() * 2].chunks_exact_mut(2) {
        unit.copy_from_slice(&1u16.to_ne_bytes())
    }
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::MissingNul, Some("wide_c"));
    let mut b = encoded();
    b.as_bytes_mut()[o..o + 2].copy_from_slice(&0u16.to_ne_bytes());
    b.as_bytes_mut()[o + 2..o + 4].copy_from_slice(&9u16.to_ne_bytes());
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::NonZeroTail, Some("wide_c"));
    assert_eq!(
        e.to_string(),
        "Malformed.wide_c: nonzero tail at logical offset 1"
    );
}

#[test]
fn helper_and_parent_padding_precedence_is_exact() {
    let (o, s) = string("wide");
    assert!(s.data_offset() > 1);
    let mut b = encoded();
    b.as_bytes_mut()[o + 1] = 1;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::NonZeroPadding, None);
    assert_eq!(
        e.to_string(),
        format!("Malformed: nonzero padding byte at offset {}", o + 1)
    );
    let mut b = encoded();
    let r = *Malformed::LAYOUT
        .padding()
        .iter()
        .find(|r| r.start() < r.end())
        .unwrap();
    b.as_bytes_mut()[r.start()] = 1;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::NonZeroPadding, None);
    let mut b = encoded();
    b.as_bytes_mut()[field("flag").offset()] = 2;
    b.as_bytes_mut()[r.start()] = 1;
    assert_eq!(
        Malformed::parse(b.as_bytes()).unwrap_err().kind(),
        ErrorKind::InvalidBool
    );
}

#[derive(Debug, ZeroSchema)]
struct Custom {
    #[zero(validate_with=reject_seven)]
    value: u8,
}

#[test]
fn declarative_and_custom_validation_are_distinct_and_ordered() {
    let off = field("checked").offset();
    let mut b = encoded();
    b.as_bytes_mut()[off] = 10;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::RangeViolation, Some("checked"));
    let mut b = encoded();
    b.as_bytes_mut()[off] = 6;
    let e = Malformed::parse(b.as_bytes()).unwrap_err();
    assert_leaf(&e, ErrorKind::MustEqualViolation, Some("checked"));
    let mut c = zero_schema::make_buffer_for!(Custom);
    Custom { value: 1 }.encode_into(c.as_bytes_mut()).unwrap();
    c.as_bytes_mut()[0] = 7;
    let e = Custom::parse(c.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::CustomValidation);
    assert_eq!(e.segment(), Some(ErrorPathSegment::Field("value")));
    assert_eq!(e.validation_code(), Some(707));
    assert_eq!(
        e.source()
            .unwrap()
            .downcast_ref::<ValidationFailure>()
            .unwrap()
            .code(),
        707
    );
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Payload {
    value: u32,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Tag {
    Empty = 1,
    Data = 2,
    Spare = 3,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag=Tag,tail="zero",padding="zero")]
enum Union {
    #[zero(tag=Tag::Empty)]
    Empty,
    #[zero(tag=Tag::Data)]
    Data(Payload),
}
#[test]
fn internal_tag_selected_payload_and_inactive_tail_precedence() {
    let mut b = zero_schema::make_buffer_for!(Union);
    Union::Empty.encode_into(b.as_bytes_mut()).unwrap();
    b.as_bytes_mut()[0] = 99;
    let e = Union::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnknownUnionTag);
    b.as_bytes_mut()[0] = 3;
    assert_eq!(
        Union::parse(b.as_bytes()).unwrap_err().kind(),
        ErrorKind::UnknownUnionTag
    );
    let (po, _) = match Union::LAYOUT.kind() {
        TypeKind::TaggedUnion {
            payload_offset,
            payload_size,
            ..
        } => (payload_offset, payload_size),
        _ => panic!(),
    };
    let mut b = zero_schema::make_buffer_for!(Union);
    Union::Empty.encode_into(b.as_bytes_mut()).unwrap();
    b.as_bytes_mut()[po + 1] = 1;
    let e = Union::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::NonZeroTail);
    assert_eq!(
        e.to_string(),
        "Union: nonzero inactive payload byte at offset 1"
    );
    let mut b = zero_schema::make_buffer_for!(Union);
    Union::Data(Payload { value: 9 })
        .encode_into(b.as_bytes_mut())
        .unwrap();
    assert_eq!(
        Union::parse(b.as_bytes()).unwrap(),
        Union::Data(Payload { value: 9 })
    );
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct ExternalEnvelope {
    #[zero(tag_field=tag)]
    payload: Union,
    intervening: bool,
    tag: Tag,
}

fn external() -> zero_schema::AlignedBytes<
    <ExternalEnvelope as zero_schema::ZeroSchemaType>::Wire,
    { ExternalEnvelope::WIRE_SIZE },
> {
    let mut b = zero_schema::make_buffer_for!(ExternalEnvelope);
    ExternalEnvelope {
        payload: Union::Data(Payload { value: 9 }),
        intervening: true,
        tag: Tag::Data,
    }
    .encode_into(b.as_bytes_mut())
    .unwrap();
    b
}

#[test]
fn external_preread_unknown_and_known_unmapped_have_exact_paths_and_precedence() {
    let tag = ExternalEnvelope::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "tag")
        .unwrap()
        .offset();
    let intervening = ExternalEnvelope::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "intervening")
        .unwrap()
        .offset();
    let mut b = external();
    b.as_bytes_mut()[tag] = 99;
    b.as_bytes_mut()[intervening] = 2;
    let e = ExternalEnvelope::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnknownEnumValue);
    assert_eq!(e.schema(), "ExternalEnvelope");
    assert_eq!(e.segment(), Some(ErrorPathSegment::Field("tag")));
    assert_eq!(e.child().unwrap().schema(), "Tag");
    assert_eq!(e.to_string(), "ExternalEnvelope.tag: unknown enum value 99");
    let mut b = external();
    b.as_bytes_mut()[tag] = 3;
    b.as_bytes_mut()[intervening] = 2;
    let e = ExternalEnvelope::parse(b.as_bytes()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnknownUnionTag);
    assert_eq!(e.segment(), Some(ErrorPathSegment::Field("payload")));
    assert_eq!(e.child().unwrap().schema(), "Union");
    assert_eq!(
        e.to_string(),
        "ExternalEnvelope.payload: unknown union tag 3"
    );
}
