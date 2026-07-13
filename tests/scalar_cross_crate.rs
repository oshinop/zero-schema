use zero_schema::{Endian, ErrorKind, IntegerRepr, LayoutError, TypeKind, ZeroSchemaType};
use zero_schema_cross_crate_child::{BigCode, LittleCode, NativeCode};
use zero_schema_cross_crate_consumer::{
    big_layout_facts, big_unknown_facts, encode_big, encode_little, encode_native, parse_big,
    parse_little_prefix,
};

fn requires_only_schema<T: ZeroSchemaType>() {}

#[repr(align(4))]
struct Aligned<const N: usize>([u8; N]);

#[test]
fn public_derive_only_values_cross_the_crate_boundary() {
    requires_only_schema::<BigCode>();
    requires_only_schema::<NativeCode>();

    assert_eq!(encode_big(BigCode::Ready), [0x01, 0x02]);
    assert_eq!(encode_big(BigCode::r#type), [0xab, 0xcd]);
    assert_eq!(encode_little(LittleCode::First), [0x04, 0x03, 0x02, 0x01]);
    assert_eq!(
        encode_native(NativeCode::Marker),
        0x1122_3344u32.to_ne_bytes()
    );

    let big = Aligned([0xab, 0xcd]);
    assert_eq!(parse_big(&big.0), Ok(0xabcd));

    let prefixed = Aligned([0x04, 0x03, 0x02, 0x01, 9, 8]);
    let (value, rest) = parse_little_prefix(&prefixed.0).unwrap();
    assert_eq!(value, 0x0102_0304);
    assert_eq!(rest, &[9, 8]);
}

#[test]
fn child_metadata_is_public_ordered_and_normalizes_raw_names() {
    assert_eq!(BigCode::WIRE_SIZE, 2);
    assert_eq!(BigCode::WIRE_ALIGN, 2);
    assert_eq!(BigCode::WIRE_STRIDE, 2);
    assert_eq!(BigCode::LAYOUT.name(), "BigCode");
    assert_eq!(
        BigCode::LAYOUT.kind(),
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U16,
            endian: Endian::Big,
        }
    );
    let values = BigCode::LAYOUT.enum_values();
    assert_eq!(values.len(), 2);
    assert_eq!((values[0].name(), values[0].raw_value()), ("Ready", 0x0102));
    assert_eq!((values[1].name(), values[1].raw_value()), ("type", 0xabcd));

    assert_eq!(
        LittleCode::LAYOUT.kind(),
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U32,
            endian: Endian::Little,
        }
    );
    assert_eq!(
        NativeCode::LAYOUT.kind(),
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U32,
            endian: Endian::Native,
        }
    );
}

#[test]
fn consumer_exposes_exact_structured_error_facts() {
    let unknown_bytes = Aligned([0, 3]);
    let unknown = big_unknown_facts(&unknown_bytes.0);
    assert_eq!(unknown.display, "BigCode: unknown enum value 3");
    assert_eq!(unknown.kind, ErrorKind::UnknownEnumValue);
    assert_eq!(unknown.schema, "BigCode");
    assert_eq!(unknown.source, None);

    let short = Aligned([0]);
    let layout = big_layout_facts(&short.0);
    assert_eq!(
        layout.display,
        "BigCode: incorrect size: expected 2 bytes, got 1"
    );
    assert_eq!(layout.kind, ErrorKind::Layout);
    assert_eq!(layout.schema, "BigCode");
    assert_eq!(
        layout.source,
        Some(LayoutError::IncorrectSize {
            expected: 2,
            actual: 1,
        })
    );
}
