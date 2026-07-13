use core::ffi::CStr;
use widestring::{U16CStr, U16Str};
use zero_schema::{
    Endian, ErrorKind, ErrorPathSegment, FieldKind, LengthRepr, SchemaError, StringEncoding,
    TailPolicy, ZeroSchema,
};

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(endian = "big")]
struct Direct<'a> {
    u: u32,
    i: i16,
    f: f32,
    flag: bool,
    #[zero(capacity = 5, len_type = u8, tail = "zero")]
    text: &'a str,
    #[zero(capacity = 4, tail = "zero")]
    c: &'a CStr,
    #[zero(capacity = 3, len_type = u8, endian = "native", tail = "zero")]
    wide: &'a U16Str,
    #[zero(capacity = 3, endian = "native", tail = "zero")]
    wide_c: &'a U16CStr,
    bytes: &'a [u8; 3],
}

fn value<'a>(text: &'a str, c: &'a CStr, wide: &'a U16Str, wide_c: &'a U16CStr) -> Direct<'a> {
    Direct {
        u: 0x1234_5678,
        i: -7,
        f: f32::from_bits(0x7fc0_1234),
        flag: true,
        text,
        c,
        wide,
        wide_c,
        bytes: &[7, 8, 9],
    }
}

#[test]
fn every_direct_field_round_trips_and_prefix_is_exact() {
    let c = c"ab";
    let wu = [0xd800];
    let wide = U16Str::from_slice(&wu);
    let wcu = [65, 0];
    let wide_c = U16CStr::from_slice(&wcu).unwrap();
    let original = value("hey", c, wide, wide_c);
    let mut buffer = zero_schema::make_buffer_for!(Direct);
    original.encode_into(buffer.as_bytes_mut()).unwrap();
    let parsed = Direct::parse(buffer.as_bytes()).unwrap();
    assert_eq!(parsed.u, original.u);
    assert_eq!(parsed.i, original.i);
    assert_eq!(parsed.f.to_bits(), original.f.to_bits());
    assert_eq!(parsed.text, "hey");
    assert_eq!(parsed.c, c);
    assert_eq!(parsed.wide, wide);
    assert_eq!(parsed.wide_c, wide_c);
    assert_eq!(parsed.bytes, &[7, 8, 9]);
    let mut prefixed = vec![0u8; Direct::WIRE_SIZE + 2];
    prefixed[..Direct::WIRE_SIZE].copy_from_slice(buffer.as_bytes());
    let (_, rest) = Direct::parse_prefix(&prefixed).unwrap();
    assert_eq!(rest.len(), 2);
}

#[test]
fn encode_failure_is_transactional() {
    let c = c"ab";
    let wide = U16Str::from_slice(&[]);
    let wcu = [0];
    let wide_c = U16CStr::from_slice(&wcu).unwrap();
    let too_long = value("123456", c, wide, wide_c);
    let mut buffer = zero_schema::make_buffer_for!(Direct);
    buffer.as_bytes_mut().fill(0xa5);
    let before = buffer.as_bytes().to_vec();
    let error = too_long.encode_into(buffer.as_bytes_mut()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(error.schema(), "Direct");
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("text")));
    assert_eq!(
        error.to_string(),
        "Direct.text: length 6 exceeds encoding capacity 5"
    );
    assert!(core::error::Error::source(&error).is_none());
    assert_eq!(buffer.as_bytes(), before);
}

#[test]
fn malformed_bool_has_structured_leaf_and_precedes_later_fields() {
    let c = c"ab";
    let wide = U16Str::from_slice(&[]);
    let wcu = [0];
    let wide_c = U16CStr::from_slice(&wcu).unwrap();
    let mut buffer = zero_schema::make_buffer_for!(Direct);
    value("hey", c, wide, wide_c)
        .encode_into(buffer.as_bytes_mut())
        .unwrap();
    let flag_offset = Direct::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "flag")
        .unwrap()
        .offset();
    buffer.as_bytes_mut()[flag_offset] = 2;
    let error = Direct::parse(buffer.as_bytes()).unwrap_err();
    assert!(matches!(
        error,
        DirectDecodeError::InvalidBool {
            field: "flag",
            value: 2
        }
    ));
    assert_eq!(error.kind(), ErrorKind::InvalidBool);
    assert_eq!(error.schema(), "Direct");
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("flag")));
    assert_eq!(
        error.to_string(),
        "Direct.flag: invalid boolean value 2; expected 0 or 1"
    );
    assert!(core::error::Error::source(&error).is_none());
}

#[test]
fn layout_metadata_and_padding_are_exact_and_ordered() {
    let layout = Direct::LAYOUT;
    assert_eq!(layout.size(), Direct::WIRE_SIZE);
    assert_eq!(layout.align(), Direct::WIRE_ALIGN);
    assert_eq!(layout.stride(), Direct::WIRE_STRIDE);
    assert_eq!(
        Direct::WIRE_STRIDE,
        (Direct::WIRE_SIZE + Direct::WIRE_ALIGN - 1) & !(Direct::WIRE_ALIGN - 1)
    );
    assert_eq!(layout.fields().len(), 9);
    for (index, field) in layout.fields().iter().enumerate() {
        assert_eq!(field.declaration_index(), index);
        assert_eq!(field.offset() % field.align(), 0);
        assert!(field.offset() + field.size() <= Direct::WIRE_SIZE);
    }
    let string = |name| match layout
        .fields()
        .iter()
        .find(|f| f.name() == name)
        .unwrap()
        .kind()
    {
        FieldKind::String(value) => value,
        _ => panic!("expected string descriptor"),
    };
    let text = string("text");
    assert_eq!(
        (
            text.encoding(),
            text.unit_endian(),
            text.capacity(),
            text.data_offset(),
            text.tail()
        ),
        (StringEncoding::Utf8, None, 5, 1, TailPolicy::Zero)
    );
    let text_len = text.length().unwrap();
    assert_eq!(
        (text_len.repr(), text_len.endian(), text_len.offset()),
        (LengthRepr::U8, Endian::Big, 0)
    );
    let c = string("c");
    assert_eq!(
        (
            c.encoding(),
            c.unit_endian(),
            c.capacity(),
            c.length(),
            c.data_offset(),
            c.tail()
        ),
        (StringEncoding::CBytes, None, 4, None, 0, TailPolicy::Zero)
    );
    let wide = string("wide");
    assert_eq!(
        (
            wide.encoding(),
            wide.unit_endian(),
            wide.capacity(),
            wide.data_offset(),
            wide.tail()
        ),
        (
            StringEncoding::U16,
            Some(Endian::Native),
            3,
            2,
            TailPolicy::Zero
        )
    );
    assert_eq!(wide.length().unwrap().repr(), LengthRepr::U8);
    let wide_c = string("wide_c");
    assert_eq!(
        (
            wide_c.encoding(),
            wide_c.unit_endian(),
            wide_c.capacity(),
            wide_c.length(),
            wide_c.data_offset(),
            wide_c.tail()
        ),
        (
            StringEncoding::U16C,
            Some(Endian::Native),
            3,
            None,
            0,
            TailPolicy::Zero
        )
    );
    for pair in layout.padding().windows(2) {
        assert!(pair[0].end() <= pair[1].start());
    }
    for range in layout.padding() {
        assert!(range.start() <= range.end() && range.end() <= Direct::WIRE_SIZE);
    }
    assert!(
        layout
            .padding()
            .iter()
            .any(|range| range.start() == layout.fields()[6].offset() + 1
                && range.end() == layout.fields()[6].offset() + 2)
    );
}
