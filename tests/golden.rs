use core::ffi::CStr;
use sha2::{Digest, Sha256};
use zero_schema_schema_corpus::*;

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_golden(actual: &[u8], expected: &[u8], digest: &str) {
    assert_eq!(actual, expected);
    assert_eq!(hex(Sha256::digest(expected)), digest);
}

#[test]
fn scalar_endian_goldens_are_exact() {
    let mut a = zero_schema::make_buffer_for!(CorpusCode8);
    CorpusCode8::Max.encode_into(a.as_bytes_mut()).unwrap();
    assert_golden(
        a.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/code8-max.bin"),
        "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89",
    );
    let mut b = zero_schema::make_buffer_for!(CorpusCode16Be);
    CorpusCode16Be::Marker
        .encode_into(b.as_bytes_mut())
        .unwrap();
    assert_golden(
        b.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/code16be-marker.bin"),
        "3a103a4e5729ad68c02a678ae39accfbc0ae208096437401b7ceab63cca0622f",
    );
}

#[test]
fn struct_and_string_goldens_are_exact() {
    let mut a = zero_schema::make_buffer_for!(EndianMatrix);
    EndianMatrix {
        byte: 0xab,
        little: 0x1234,
        big: 0x0102_0304,
    }
    .encode_into(a.as_bytes_mut())
    .unwrap();
    assert_golden(
        a.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/endian-matrix.bin"),
        "0ab9195c554ceab6dd16a28790b26fc8ddebfe12b7be632182f2e4c88b5b592e",
    );
    let mut b = zero_schema::make_buffer_for!(StringMatrix);
    StringMatrix {
        text: "A\0B",
        c_text: CStr::from_bytes_with_nul(&[0xff, 0x7f, 0]).unwrap(),
    }
    .encode_into(b.as_bytes_mut())
    .unwrap();
    assert_golden(
        b.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/string-matrix.bin"),
        "d5c25ee85c238853edab7f5acd53ba51eea3c47bd47a53f0ebc965c09534d359",
    );
}

#[test]
fn union_goldens_are_exact() {
    let mut a = zero_schema::make_buffer_for!(CorpusMessage);
    CorpusMessage::Unit.encode_into(a.as_bytes_mut()).unwrap();
    assert_golden(
        a.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/message-unit.bin"),
        "7c9fa136d4413fa6173637e883b6998d32e1d675f88cddff9dcbcf331820f4b8",
    );
    let mut b = zero_schema::make_buffer_for!(CorpusMessage);
    CorpusMessage::Payload(CorpusPayload { value: 0x1122_3344 })
        .encode_into(b.as_bytes_mut())
        .unwrap();
    assert_golden(
        b.as_bytes(),
        include_bytes!("../test-fixtures/schema-corpus/golden/message-payload.bin"),
        "07a7d9171fa94f3bef0d7dccfc26aaa2b7508fcf2181a79df193a031cc5c60d4",
    );
}

#[test]
fn layout_constants_are_frozen() {
    assert_eq!((CorpusCode8::WIRE_SIZE, CorpusCode8::WIRE_ALIGN), (1, 1));
    assert_eq!(
        (CorpusCode16Be::WIRE_SIZE, CorpusCode16Be::WIRE_ALIGN),
        (2, 2)
    );
    assert_eq!((EndianMatrix::WIRE_SIZE, EndianMatrix::WIRE_ALIGN), (8, 4));
    assert_eq!((StringMatrix::WIRE_SIZE, StringMatrix::WIRE_ALIGN), (8, 1));
    assert_eq!(
        (CorpusMessage::WIRE_SIZE, CorpusMessage::WIRE_ALIGN),
        (8, 4)
    );
}

#[test]
fn malformed_error_contract_is_frozen() {
    use zero_schema::{ErrorKind, SchemaError};
    let mut scalar = zero_schema::make_buffer_for!(CorpusCode8);
    scalar.as_bytes_mut()[0] = 7;
    let error = CorpusCode8::parse(scalar.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnknownEnumValue);
    assert_eq!(error.schema(), "CorpusCode8");
    assert_eq!(error.to_string(), "CorpusCode8: unknown enum value 7");

    let mut message = zero_schema::make_buffer_for!(CorpusMessage);
    message.as_bytes_mut()[0] = 99;
    let error = CorpusMessage::parse(message.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnknownUnionTag);
    assert_eq!(error.schema(), "CorpusMessage");
    assert_eq!(error.to_string(), "CorpusMessage: unknown union tag 99");
}
