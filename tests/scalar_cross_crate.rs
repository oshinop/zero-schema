use zero_schema::{Endian, ErrorKind, IntegerRepr, LayoutError, TypeKind};
use zero_schema_cross_crate_child::{BigCode, LittleCode, NativeCode};
use zero_schema_cross_crate_consumer::{
    big_layout_facts, big_unknown_facts, read_big, read_little,
};

#[repr(align(4))]
struct Aligned<const N: usize>([u8; N]);

#[test]
fn public_scalar_capabilities_cross_the_crate_boundary() {
    let big = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-child/golden/big-ready.bin"
    ));
    assert_eq!(read_big(&big.0), Ok(0x0102));

    let little = Aligned(*include_bytes!(
        "../test-fixtures/cross-crate-child/golden/little-first.bin"
    ));
    assert_eq!(read_little(&little.0), Ok(0x0102_0304));

    assert_eq!(
        (
            BigCode::SCHEMA_SIZE,
            BigCode::SCHEMA_ALIGN,
            BigCode::SCHEMA_STRIDE
        ),
        (2, 2, 2)
    );
    assert_eq!(NativeCode::SCHEMA_SIZE, 4);
}

#[test]
fn child_metadata_is_public_ordered_and_normalizes_raw_names() {
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
fn consumer_exposes_structured_access_error_facts() {
    let unknown_bytes = Aligned([0, 3]);
    let unknown = big_unknown_facts(&unknown_bytes.0);
    assert_eq!(unknown.display, "BigCode: unknown scalar enum value");
    assert_eq!(unknown.kind, ErrorKind::UnknownEnumValue);
    assert_eq!(unknown.schema, "BigCode");
    assert_eq!(unknown.source, None);

    let short = Aligned([0]);
    let layout = big_layout_facts(&short.0);
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
