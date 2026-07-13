use widestring::U16Str;
use zero_schema::{ErrorKind, FieldKind, SchemaError, ZeroSchema, ZeroSchemaType};

#[derive(ZeroSchema)]
#[zero(align = 16, padding = "zero")]
struct Aligned<'a> {
    lead: u8,
    #[zero(align = 8)]
    word: u32,
    #[zero(capacity = 3, len_type = u8)]
    text: &'a str,
}

#[derive(ZeroSchema)]
#[zero(padding = "zero")]
struct HelperPads<'a> {
    #[zero(capacity = 1, len_type = u8, align = 8)]
    utf8: &'a str,
    #[zero(capacity = 1, len_type = u32, endian = "native")]
    wide: &'a U16Str,
}

#[derive(ZeroSchema)]
#[zero(padding = "zero")]
struct WidePrefixGap<'a> {
    #[zero(capacity = 1, len_type = u8, endian = "native")]
    wide: &'a U16Str,
}

#[test]
fn descriptor_matches_lowered_wire_layout() {
    assert_eq!(
        Aligned::WIRE_SIZE,
        core::mem::size_of::<<Aligned<'static> as ZeroSchemaType>::Wire>()
    );
    assert_eq!(Aligned::WIRE_ALIGN, 16);
    assert_eq!(Aligned::LAYOUT.size(), Aligned::WIRE_SIZE);
    assert_eq!(Aligned::LAYOUT.align(), Aligned::WIRE_ALIGN);
    let fields = Aligned::LAYOUT.fields();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].offset(), 0);
    assert_eq!(fields[1].offset() % 8, 0);
    assert!(matches!(fields[2].kind(), FieldKind::String(_)));
    for range in Aligned::LAYOUT.padding() {
        assert!(range.start() <= range.end());
        assert!(range.end() <= Aligned::WIRE_SIZE);
    }
}

#[test]
fn aligned_layout_round_trips() {
    let value = Aligned {
        lead: 7,
        word: 0x1122_3344,
        text: "abc",
    };
    let mut buffer = zero_schema::make_buffer_for!(Aligned);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded = Aligned::parse(buffer.as_bytes()).unwrap();
    assert_eq!(
        (decoded.lead, decoded.word, decoded.text),
        (7, 0x1122_3344, "abc")
    );
}

#[test]
fn helper_and_wrapper_padding_ranges_are_complete_and_ordered() {
    let fields = HelperPads::LAYOUT.fields();
    let utf8 = match fields[0].kind() {
        FieldKind::String(value) => value,
        _ => unreachable!(),
    };
    let wide = match fields[1].kind() {
        FieldKind::String(value) => value,
        _ => unreachable!(),
    };
    let nonempty: Vec<_> = HelperPads::LAYOUT
        .padding()
        .iter()
        .filter(|range| range.start() != range.end())
        .map(|range| (range.start(), range.end()))
        .collect();
    assert!(nonempty.contains(&(
        fields[0].offset() + utf8.data_offset() + 1,
        fields[0].offset() + fields[0].size()
    )));
    assert!(nonempty.contains(&(
        fields[1].offset() + wide.data_offset() + 2,
        fields[1].offset() + fields[1].size()
    )));
    for pair in HelperPads::LAYOUT.padding().windows(2) {
        assert!(pair[0].end() <= pair[1].start());
    }
    let prefix_field = &WidePrefixGap::LAYOUT.fields()[0];
    let prefix = match prefix_field.kind() {
        FieldKind::String(value) => value,
        _ => unreachable!(),
    };
    assert!(WidePrefixGap::LAYOUT.padding().iter().any(|range| {
        range.start() == prefix_field.offset() + 1
            && range.end() == prefix_field.offset() + prefix.data_offset()
    }));
}

#[test]
fn padding_decode_reports_the_first_owned_byte() {
    let units = [65u16];
    let value = HelperPads {
        utf8: "x",
        wide: U16Str::from_slice(&units),
    };
    let mut buffer = zero_schema::make_buffer_for!(HelperPads<'static>);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let first = HelperPads::LAYOUT
        .padding()
        .iter()
        .find(|range| range.start() != range.end())
        .unwrap()
        .start();
    buffer.as_bytes_mut()[first] = 0x5a;
    let error = match HelperPads::parse(buffer.as_bytes()) {
        Ok(_) => panic!("expected padding failure"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ErrorKind::NonZeroPadding);
    assert_eq!(
        error.to_string(),
        format!("HelperPads: nonzero padding byte at offset {first}")
    );
}
