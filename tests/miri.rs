use core::error::Error as _;
use core::ffi::CStr;

use widestring::{U16CStr, U16Str};
use zero_schema::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError, ZeroSchema};

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(align = 8, padding = "zero")]
struct Borrowed<'a> {
    marker: u8,
    #[zero(capacity = 8, len_type = u8, tail = "zero")]
    utf8: &'a str,
    #[zero(capacity = 6, tail = "zero")]
    c: &'a CStr,
    #[zero(capacity = 5, len_type = u8, endian = "native", tail = "zero")]
    wide: &'a U16Str,
    #[zero(capacity = 5, endian = "native", tail = "zero")]
    wide_c: &'a U16CStr,
}

#[derive(Clone, Copy, Debug, PartialEq, ZeroSchema)]
struct Payload {
    valid: bool,
    value: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Nested {
    payload: Payload,
}

#[derive(Clone, Copy, Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Tag {
    Unit = 1,
    Data = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag, tail = "zero")]
enum Message {
    #[zero(tag = Tag::Unit)]
    Unit,
    #[zero(tag = Tag::Data)]
    Data(Payload),
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct External {
    tag: Tag,
    #[zero(tag_field = tag)]
    message: Message,
}

fn assert_points_into<T>(pointer: *const T, units: usize, source: &[u8]) {
    let start = source.as_ptr() as usize;
    let end = start + source.len();
    let pointer = pointer as usize;
    let byte_len = units * core::mem::size_of::<T>();
    assert!(pointer >= start);
    assert!(pointer + byte_len <= end);
}

fn borrowed_value<'a>(c: &'a CStr, wide: &'a U16Str, wide_c: &'a U16CStr) -> Borrowed<'a> {
    Borrowed {
        marker: 7,
        utf8: "a\0z",
        c,
        wide,
        wide_c,
    }
}

fn field(
    name: &str,
    layout: &'static zero_schema::LayoutDescriptor,
) -> &'static zero_schema::FieldDescriptor {
    layout
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .unwrap()
}

fn assert_direct_field_error(error: &dyn SchemaError, kind: ErrorKind, name: &'static str) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field(name)));
    assert!(error.child().is_none());
}

#[test]
fn generated_buffer_borrows_and_exact_prefix_roundtrip() {
    let c = c"c\xff";
    let wide_units = [0xd800, 0x61];
    let wide = U16Str::from_slice(&wide_units);
    let wide_c_units = [0xdc00, 0x62, 0];
    let wide_c = U16CStr::from_slice(&wide_c_units).unwrap();
    let value = borrowed_value(c, wide, wide_c);

    let mut buffer = zero_schema::make_buffer_for!(Borrowed<'static>);
    assert!(buffer.as_bytes().iter().all(|byte| *byte == 0));
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded = Borrowed::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded, value);
    assert_points_into(decoded.utf8.as_ptr(), decoded.utf8.len(), buffer.as_bytes());
    assert_points_into(
        decoded.c.as_ptr(),
        decoded.c.to_bytes_with_nul().len(),
        buffer.as_bytes(),
    );
    assert_points_into(decoded.wide.as_ptr(), decoded.wide.len(), buffer.as_bytes());
    assert_points_into(
        decoded.wide_c.as_ptr(),
        decoded.wide_c.as_slice_with_nul().len(),
        buffer.as_bytes(),
    );

    #[repr(align(8))]
    struct Prefix([u8; Borrowed::WIRE_SIZE + 3]);
    let mut prefixed = Prefix([0u8; Borrowed::WIRE_SIZE + 3]);
    prefixed.0[..Borrowed::WIRE_SIZE].copy_from_slice(buffer.as_bytes());
    prefixed.0[Borrowed::WIRE_SIZE..].copy_from_slice(&[9, 8, 7]);
    let (decoded, rest) = Borrowed::parse_prefix(&prefixed.0).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(rest, &[9, 8, 7]);
    assert_points_into(decoded.utf8.as_ptr(), decoded.utf8.len(), &prefixed.0);
}

#[test]
fn alignment_and_original_padding_bytes_are_checked() {
    let mut aligned = zero_schema::make_buffer_for!(Borrowed<'static>);
    borrowed_value(
        c"ok",
        U16Str::from_slice(&[1]),
        U16CStr::from_slice(&[2, 0]).unwrap(),
    )
    .encode_into(aligned.as_bytes_mut())
    .unwrap();

    #[repr(align(16))]
    struct Storage([u8; Borrowed::WIRE_SIZE + 1]);
    let mut storage = Storage([0; Borrowed::WIRE_SIZE + 1]);
    storage.0[1..].copy_from_slice(aligned.as_bytes());
    let decode = Borrowed::parse(&storage.0[1..]).unwrap_err();
    assert_eq!(decode.kind(), ErrorKind::Layout);
    assert!(matches!(
        decode
            .source()
            .and_then(|e| e.downcast_ref::<LayoutError>()),
        Some(LayoutError::Misaligned { .. })
    ));
    let encode = borrowed_value(
        c"ok",
        U16Str::from_slice(&[1]),
        U16CStr::from_slice(&[2, 0]).unwrap(),
    )
    .encode_into(&mut storage.0[1..])
    .unwrap_err();
    assert_eq!(encode.kind(), ErrorKind::Layout);

    let padding = Borrowed::LAYOUT
        .padding()
        .iter()
        .find(|range| range.start() < range.end())
        .unwrap()
        .start();
    aligned.as_bytes_mut()[padding] = 0x5a;
    let error = Borrowed::parse(aligned.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NonZeroPadding);
    assert!(error.to_string().contains(&format!("offset {padding}")));
}

#[test]
fn malformed_borrowed_strings_report_deterministic_errors() {
    let value = borrowed_value(
        c"ok",
        U16Str::from_slice(&[1]),
        U16CStr::from_slice(&[2, 0]).unwrap(),
    );
    let mut valid = zero_schema::make_buffer_for!(Borrowed<'static>);
    value.encode_into(valid.as_bytes_mut()).unwrap();

    let utf8 = field("utf8", Borrowed::LAYOUT);
    let utf8_data = match utf8.kind() {
        zero_schema::FieldKind::String(string) => string.data_offset(),
        _ => unreachable!(),
    };
    let mut bad_utf8 = zero_schema::make_buffer_for!(Borrowed<'static>);
    bad_utf8.as_bytes_mut().copy_from_slice(valid.as_bytes());
    bad_utf8.as_bytes_mut()[utf8.offset() + utf8_data] = 0xff;
    let error = Borrowed::parse(bad_utf8.as_bytes()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::InvalidUtf8, "utf8");

    let wide = field("wide", Borrowed::LAYOUT);
    let wide_length = match wide.kind() {
        zero_schema::FieldKind::String(string) => string.length().unwrap().offset(),
        _ => unreachable!(),
    };
    let mut excessive_wide = zero_schema::make_buffer_for!(Borrowed<'static>);
    excessive_wide
        .as_bytes_mut()
        .copy_from_slice(valid.as_bytes());
    excessive_wide.as_bytes_mut()[wide.offset() + wide_length] = 6;
    let error = Borrowed::parse(excessive_wide.as_bytes()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::LengthOutOfBounds, "wide");

    let wide_c = field("wide_c", Borrowed::LAYOUT);
    let wide_c_data = match wide_c.kind() {
        zero_schema::FieldKind::String(string) => string.data_offset(),
        _ => unreachable!(),
    };
    let mut missing_nul = zero_schema::make_buffer_for!(Borrowed<'static>);
    missing_nul.as_bytes_mut().copy_from_slice(valid.as_bytes());
    for unit in 0..5 {
        let start = wide_c.offset() + wide_c_data + unit * 2;
        missing_nul.as_bytes_mut()[start..start + 2].copy_from_slice(&1u16.to_ne_bytes());
    }
    let error = Borrowed::parse(missing_nul.as_bytes()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::MissingNul, "wide_c");

    let wide_data = match wide.kind() {
        zero_schema::FieldKind::String(string) => string.data_offset(),
        _ => unreachable!(),
    };
    let mut wide_tail = zero_schema::make_buffer_for!(Borrowed<'static>);
    wide_tail.as_bytes_mut().copy_from_slice(valid.as_bytes());
    wide_tail.as_bytes_mut()[wide.offset() + wide_data + 2..wide.offset() + wide_data + 4]
        .copy_from_slice(&9u16.to_ne_bytes());
    let error = Borrowed::parse(wide_tail.as_bytes()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::NonZeroTail, "wide");

    let mut wide_c_tail = zero_schema::make_buffer_for!(Borrowed<'static>);
    wide_c_tail.as_bytes_mut().copy_from_slice(valid.as_bytes());
    wide_c_tail.as_bytes_mut()
        [wide_c.offset() + wide_c_data + 4..wide_c.offset() + wide_c_data + 6]
        .copy_from_slice(&9u16.to_ne_bytes());
    let error = Borrowed::parse(wide_c_tail.as_bytes()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::NonZeroTail, "wide_c");
}

#[test]
fn semantic_encode_preflight_preserves_the_entire_destination() {
    let too_wide = U16Str::from_slice(&[1, 2, 3, 4, 5, 6]);
    let value = borrowed_value(c"ok", too_wide, U16CStr::from_slice(&[2, 0]).unwrap());
    let mut destination = zero_schema::make_buffer_for!(Borrowed<'static>);
    destination.as_bytes_mut().fill(0xa5);
    let mut before = zero_schema::make_buffer_for!(Borrowed<'static>);
    before
        .as_bytes_mut()
        .copy_from_slice(destination.as_bytes());
    let error = value.encode_into(destination.as_bytes_mut()).unwrap_err();
    assert_direct_field_error(&error, ErrorKind::CapacityExceeded, "wide");
    assert_eq!(destination.as_bytes(), before.as_bytes());
}

#[test]
fn selected_internal_and_external_union_inputs_roundtrip() {
    let payload = Payload {
        valid: true,
        value: 0x1020_3040,
    };
    let mut internal = zero_schema::make_buffer_for!(Message);
    Message::Data(payload)
        .encode_into(internal.as_bytes_mut())
        .unwrap();
    assert_eq!(
        Message::parse(internal.as_bytes()).unwrap(),
        Message::Data(payload)
    );

    let external = External {
        tag: Tag::Data,
        message: Message::Data(payload),
    };
    let mut buffer = zero_schema::make_buffer_for!(External);
    external.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(External::parse(buffer.as_bytes()).unwrap(), external);
}

#[test]
fn tagged_decode_errors_preserve_structured_paths_and_sources() {
    let payload = Payload {
        valid: true,
        value: 7,
    };
    let mut internal = zero_schema::make_buffer_for!(Message);
    Message::Data(payload)
        .encode_into(internal.as_bytes_mut())
        .unwrap();
    let kind = Message::LAYOUT.kind();
    let tag_offset = match kind {
        zero_schema::TypeKind::TaggedUnion { tag_offset, .. } => tag_offset,
        _ => unreachable!(),
    };
    internal.as_bytes_mut()[tag_offset] = 0xff;
    let unknown = Message::parse(internal.as_bytes()).unwrap_err();
    assert_eq!(unknown.kind(), ErrorKind::UnknownUnionTag);
    assert_eq!(unknown.schema(), "Message");
    assert_eq!(unknown.segment(), None);
    assert!(unknown.child().is_none());

    Message::Data(payload)
        .encode_into(internal.as_bytes_mut())
        .unwrap();
    let payload_offset = match Message::LAYOUT.kind() {
        zero_schema::TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => unreachable!(),
    };
    internal.as_bytes_mut()[payload_offset + field("valid", Payload::LAYOUT).offset()] = 2;
    let selected = Message::parse(internal.as_bytes()).unwrap_err();
    assert_eq!(selected.kind(), ErrorKind::InvalidBool);
    assert_eq!(selected.segment(), Some(ErrorPathSegment::Variant("Data")));
    let payload_error = selected.child().unwrap();
    assert_eq!(payload_error.schema(), "Payload");
    assert_eq!(
        payload_error.segment(),
        Some(ErrorPathSegment::Field("valid"))
    );
    assert_eq!(
        selected.source().unwrap() as *const dyn core::error::Error as *const (),
        payload_error as *const dyn SchemaError as *const ()
    );

    let external_value = External {
        tag: Tag::Data,
        message: Message::Data(payload),
    };
    let mut external = zero_schema::make_buffer_for!(External);
    external_value.encode_into(external.as_bytes_mut()).unwrap();
    external.as_bytes_mut()[field("tag", External::LAYOUT).offset()] = 0xff;
    let unknown = External::parse(external.as_bytes()).unwrap_err();
    assert_eq!(unknown.kind(), ErrorKind::UnknownEnumValue);
    assert_eq!(unknown.segment(), Some(ErrorPathSegment::Field("tag")));
    assert_eq!(unknown.child().unwrap().schema(), "Tag");
    assert_eq!(
        unknown.source().unwrap() as *const dyn core::error::Error as *const (),
        unknown.child().unwrap() as *const dyn SchemaError as *const ()
    );

    external_value.encode_into(external.as_bytes_mut()).unwrap();
    let message_offset = field("message", External::LAYOUT).offset();
    external.as_bytes_mut()[message_offset + field("valid", Payload::LAYOUT).offset()] = 2;
    let selected = External::parse(external.as_bytes()).unwrap_err();
    assert_eq!(selected.kind(), ErrorKind::InvalidBool);
    assert_eq!(selected.segment(), Some(ErrorPathSegment::Field("message")));
    let message_error = selected.child().unwrap();
    assert_eq!(message_error.schema(), "Message");
    assert_eq!(
        message_error.segment(),
        Some(ErrorPathSegment::Variant("Data"))
    );
    let payload_error = message_error.child().unwrap();
    assert_eq!(payload_error.schema(), "Payload");
    assert_eq!(
        payload_error.segment(),
        Some(ErrorPathSegment::Field("valid"))
    );
    assert_eq!(
        selected.source().unwrap() as *const dyn core::error::Error as *const (),
        message_error as *const dyn SchemaError as *const ()
    );
}

#[test]
fn nested_structured_error_traverses_without_losing_source() {
    let value = Nested {
        payload: Payload {
            valid: true,
            value: 4,
        },
    };
    let mut buffer = zero_schema::make_buffer_for!(Nested);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let payload_offset = Nested::LAYOUT.fields()[0].offset();
    let valid_offset = Payload::LAYOUT.fields()[0].offset();
    buffer.as_bytes_mut()[payload_offset + valid_offset] = 2;

    let error = Nested::parse(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidBool);
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("payload")));
    let child = error.child().unwrap();
    assert_eq!(child.schema(), "Payload");
    assert_eq!(child.segment(), Some(ErrorPathSegment::Field("valid")));
    let source = error.source().unwrap();
    assert_eq!(
        source as *const dyn core::error::Error as *const (),
        child as *const dyn SchemaError as *const ()
    );
    assert_eq!(
        error.to_string(),
        "Nested.payload.valid: invalid boolean value 2; expected 0 or 1"
    );
}
