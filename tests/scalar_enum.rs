use core::mem::{align_of_val, size_of_val};

use zero_schema::{ErrorKind, SchemaError, zero};

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Code8 {
    Maximum = 255,
}

#[zero(endian = "big")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Code16Be {
    Marker = 0x1234,
}

#[repr(align(2))]
struct Code16Producer([u8; 2]);

#[test]
fn scalar_enum_roots_access_checked_cxx_producer_bytes() {
    let code8 = include_bytes!("../test-fixtures/schema-corpus/golden/code8-max.bin");
    let code16 = Code16Producer(*include_bytes!(
        "../test-fixtures/schema-corpus/golden/code16be-marker.bin"
    ));
    assert_eq!(
        (
            Code8::SCHEMA_SIZE,
            Code8::SCHEMA_ALIGN,
            Code8::SCHEMA_STRIDE
        ),
        (1, 1, 1)
    );
    assert_eq!(
        (
            Code16Be::SCHEMA_SIZE,
            Code16Be::SCHEMA_ALIGN,
            Code16Be::SCHEMA_STRIDE
        ),
        (2, 2, 2)
    );
    assert_eq!((align_of_val(&code16), size_of_val(&code16)), (2, 2));

    assert_eq!(Code8::access(code8).unwrap().get(), Code8::Maximum);
    assert_eq!(
        Code16Be::access(&code16.0).unwrap().copy_into(),
        Code16Be::Marker
    );
    let mut mutable = Code16Producer(*include_bytes!(
        "../test-fixtures/schema-corpus/golden/code16be-marker.bin"
    ));
    let mut view = Code16Be::access_mut(&mut mutable.0).unwrap();
    assert_eq!(view.get(), Code16Be::Marker);
    view.set(Code16Be::Marker).unwrap();
    view.copy_from(&Code16BePatch::from(Code16Be::Marker))
        .unwrap();
    assert_eq!(view.copy_into(), Code16Be::Marker);
}

#[test]
fn scalar_enum_rejects_unreviewed_discriminants_without_panic() {
    let bad = [0u8];
    let Err(error) = Code8::access(&bad) else {
        panic!("unknown producer discriminant must fail access");
    };
    assert_eq!(error.kind(), ErrorKind::UnknownEnumValue);
}
