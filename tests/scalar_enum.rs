use core::error::Error as _;
use zero_schema::{
    Endian, ErrorKind, IntegerRepr, LayoutError, SchemaError, TypeKind, ZeroSchema, ZeroSchemaType,
};

#[derive(ZeroSchema)]
#[repr(u8)]
enum Tiny {
    Zero = 0,
    Max = 255,
}

#[derive(ZeroSchema, Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u16)]
#[zero(endian = "little")]
enum Little {
    Low = 0x0102,
    High = 0xffff,
}

#[derive(ZeroSchema, Debug, PartialEq)]
#[repr(u16)]
#[zero(endian = "big")]
enum Big {
    Low = 0x0102,
    High = 0xffff,
}

#[derive(ZeroSchema)]
#[repr(u32)]
#[zero(endian = "little")]
enum WideLittle {
    Boundary = 0x0102_0304,
    Max = 0xffff_ffff,
}

#[derive(ZeroSchema)]
#[repr(u32)]
#[zero(endian = "big")]
enum WideBig {
    Boundary = 0x0102_0304,
    Max = 0xffff_ffff,
}

#[derive(ZeroSchema)]
#[repr(u16)]
enum Native16 {
    Boundary = 0x0102,
}

#[derive(ZeroSchema)]
#[repr(u32)]
enum Native32 {
    Boundary = 0x0102_0304,
}

#[allow(non_camel_case_types)]
#[derive(ZeroSchema)]
#[repr(u8)]
enum RawNames {
    r#type = 7,
    Other = 8,
}

fn assert_no_standard_bounds<T: ZeroSchemaType>() {}

#[test]
fn derive_only_and_non_copy_enums_have_complete_api() {
    assert_no_standard_bounds::<Tiny>();
    assert_no_standard_bounds::<Big>();
    let bytes = Tiny::Max.encode().unwrap();
    assert_eq!(bytes.as_bytes(), &[255]);
    assert!(matches!(Tiny::parse(bytes.as_bytes()).unwrap(), Tiny::Max));
    assert_eq!(Tiny::Max.encoded_len(), 1);

    let mut big = zero_schema::make_buffer_for!(Big);
    Big::Low.encode_into(big.as_bytes_mut()).unwrap();
    assert_eq!(big.as_bytes(), &[1, 2]);
    assert_eq!(Big::parse(big.as_bytes()).unwrap(), Big::Low);
}

#[test]
fn every_width_endian_and_boundary_has_exact_bytes() {
    let mut little = zero_schema::make_buffer_for!(Little);
    Little::Low.encode_into(little.as_bytes_mut()).unwrap();
    assert_eq!(little.as_bytes(), &[2, 1]);
    assert_eq!(Little::parse(little.as_bytes()).unwrap(), Little::Low);

    let mut wl = zero_schema::make_buffer_for!(WideLittle);
    WideLittle::Boundary.encode_into(wl.as_bytes_mut()).unwrap();
    assert_eq!(wl.as_bytes(), &[4, 3, 2, 1]);
    let mut wb = zero_schema::make_buffer_for!(WideBig);
    WideBig::Boundary.encode_into(wb.as_bytes_mut()).unwrap();
    assert_eq!(wb.as_bytes(), &[1, 2, 3, 4]);
    WideLittle::Max.encode_into(wl.as_bytes_mut()).unwrap();
    WideBig::Max.encode_into(wb.as_bytes_mut()).unwrap();
    assert_eq!(wl.as_bytes(), &[255; 4]);
    assert_eq!(wb.as_bytes(), &[255; 4]);
    let mut n16 = zero_schema::make_buffer_for!(Native16);
    Native16::Boundary.encode_into(n16.as_bytes_mut()).unwrap();
    assert_eq!(n16.as_bytes(), &0x0102u16.to_ne_bytes());
    let mut n32 = zero_schema::make_buffer_for!(Native32);
    Native32::Boundary.encode_into(n32.as_bytes_mut()).unwrap();
    assert_eq!(n32.as_bytes(), &0x0102_0304u32.to_ne_bytes());
}

#[test]
fn prefix_consumes_wire_size_and_errors_report_source() {
    #[repr(align(4))]
    struct Aligned([u8; 8]);
    let bytes = Aligned([0x01, 0x02, 9, 8, 0, 0, 0, 0]);
    let (value, rest) = Big::parse_prefix(&bytes.0[..4]).unwrap();
    assert_eq!(value, Big::Low);
    assert_eq!(rest, &[9, 8]);

    let unknown = match Tiny::parse(&[3]) {
        Err(error) => error,
        Ok(_) => panic!("unknown value accepted"),
    };
    assert_eq!(unknown.kind(), ErrorKind::UnknownEnumValue);
    assert_eq!(unknown.schema(), "Tiny");
    assert!(unknown.source().is_none());
    assert_eq!(unknown.to_string(), "Tiny: unknown enum value 3");

    let wrong = match Tiny::parse(&[]) {
        Err(error) => error,
        Ok(_) => panic!("wrong size accepted"),
    };
    assert_eq!(wrong.kind(), ErrorKind::Layout);
    assert_eq!(
        wrong.to_string(),
        "Tiny: incorrect size: expected 1 bytes, got 0"
    );
    assert_eq!(
        wrong.source().unwrap().downcast_ref::<LayoutError>(),
        Some(&LayoutError::IncorrectSize {
            expected: 1,
            actual: 0
        })
    );

    let short = Big::parse_prefix(&bytes.0[..1]).unwrap_err();
    assert_eq!(
        short.source().unwrap().downcast_ref::<LayoutError>(),
        Some(&LayoutError::InsufficientBytes {
            required: 2,
            actual: 1
        })
    );
    let misaligned = Big::parse(&bytes.0[1..3]).unwrap_err();
    assert!(matches!(
        misaligned.source().unwrap().downcast_ref::<LayoutError>(),
        Some(LayoutError::Misaligned { required: 2, .. })
    ));
}

#[test]
fn encode_layout_failure_leaves_destination_untouched() {
    let mut wrong = [0xa5; 3];
    let error = Big::High.encode_into(&mut wrong).unwrap_err();
    assert_eq!(wrong, [0xa5; 3]);
    assert_eq!(
        error.source().unwrap().downcast_ref::<LayoutError>(),
        Some(&LayoutError::IncorrectSize {
            expected: 2,
            actual: 3
        })
    );
    #[repr(align(4))]
    struct Aligned([u8; 4]);
    let mut storage = Aligned([0xa5; 4]);
    let error = Big::High.encode_into(&mut storage.0[1..3]).unwrap_err();
    assert!(matches!(
        error.source().unwrap().downcast_ref::<LayoutError>(),
        Some(LayoutError::Misaligned { required: 2, .. })
    ));
    assert_eq!(storage.0, [0xa5; 4]);
}

#[test]
fn metadata_is_ordered_and_raw_names_are_normalized() {
    assert_eq!(Little::WIRE_SIZE, 2);
    assert_eq!(Little::WIRE_ALIGN, 2);
    assert_eq!(Little::WIRE_STRIDE, 2);
    assert_eq!(Little::LAYOUT.name(), "Little");
    assert_eq!(
        Little::LAYOUT.kind(),
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U16,
            endian: Endian::Little
        }
    );
    let values = Little::LAYOUT.enum_values();
    assert_eq!((values[0].name(), values[0].raw_value()), ("Low", 0x0102));
    assert_eq!((values[1].name(), values[1].raw_value()), ("High", 0xffff));
    let raw = RawNames::LAYOUT.enum_values();
    assert_eq!((raw[0].name(), raw[0].raw_value()), ("type", 7));
    let mut buffer = zero_schema::make_buffer_for!(RawNames);
    RawNames::r#type.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(buffer.as_bytes(), &[7]);
}
