use sha2::{Digest, Sha256};
use zero_schema::{ErrorKind, ErrorPathSegment, SchemaError};
use zero_schema_schema_corpus::*;

#[repr(align(16))]
struct Aligned<const N: usize>([u8; N]);

fn producer<const N: usize>(bytes: &'static [u8; N]) -> Aligned<N> {
    Aligned(*bytes)
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_golden(actual: &[u8], expected: &[u8], digest: &str) {
    assert_eq!(actual, expected);
    assert_eq!(hex(Sha256::digest(actual)), digest);
}

#[test]
fn scalar_endian_goldens_are_exact_reviewed_producer_bytes() {
    let code8 = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/code8-max.bin"
    ));
    assert_eq!(
        CorpusCode8::access(&code8.0).unwrap().get(),
        CorpusCode8::Max
    );
    assert_golden(
        &code8.0,
        &[0xff],
        "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89",
    );

    let code16 = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/code16be-marker.bin"
    ));
    assert_eq!(
        CorpusCode16Be::access(&code16.0).unwrap().get(),
        CorpusCode16Be::Marker
    );
    assert_golden(
        &code16.0,
        &[0x12, 0x34],
        "3a103a4e5729ad68c02a678ae39accfbc0ae208096437401b7ceab63cca0622f",
    );
}

#[test]
fn struct_and_string_goldens_are_exact_reviewed_producer_bytes() {
    let endian = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/endian-matrix.bin"
    ));
    let view = EndianMatrix::access(&endian.0).unwrap();
    assert_eq!(
        (view.byte(), view.little(), view.big()),
        (0xab, 0x1234, 0x0102_0304)
    );
    assert_golden(
        &endian.0,
        &[0xab, 0, 0x34, 0x12, 1, 2, 3, 4],
        "0ab9195c554ceab6dd16a28790b26fc8ddebfe12b7be632182f2e4c88b5b592e",
    );

    let strings = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/string-matrix.bin"
    ));
    let view = StringMatrix::access(&strings.0).unwrap();
    assert_eq!(view.text(), "A\0B");
    assert_eq!(view.c_text().to_bytes(), [0xff, 0x7f]);
    assert_golden(
        &strings.0,
        &[3, b'A', 0, b'B', 0xff, 0x7f, 0, 0],
        "d5c25ee85c238853edab7f5acd53ba51eea3c47bd47a53f0ebc965c09534d359",
    );
}

#[test]
fn external_union_goldens_are_exact_reviewed_producer_bytes() {
    let unit = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/message-unit.bin"
    ));
    let unit_view = ExternalCorpusMessage::access(&unit.0).unwrap();
    assert_eq!(unit_view.tag(), CorpusTag::Unit);
    assert!(unit_view.payload().unit().is_some());
    let _ = unit_view.copy_into();
    assert_golden(
        &unit.0,
        &[1, 0, 0, 0, 0, 0, 0, 0],
        "7c9fa136d4413fa6173637e883b6998d32e1d675f88cddff9dcbcf331820f4b8",
    );

    let payload = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/message-payload.bin"
    ));
    let payload_view = ExternalCorpusMessage::access(&payload.0).unwrap();
    assert_eq!(payload_view.tag(), CorpusTag::Payload);
    assert_eq!(
        payload_view.payload().payload().unwrap().value(),
        0x1122_3344
    );
    assert_golden(
        &payload.0,
        &[2, 0, 0, 0, 0x44, 0x33, 0x22, 0x11],
        "07a7d9171fa94f3bef0d7dccfc26aaa2b7508fcf2181a79df193a031cc5c60d4",
    );
}

#[test]
fn every_inventory_golden_accesses_its_declared_root() {
    let code16 = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/1.bin"
    ));
    assert_eq!(code16.0.len(), CorpusCode16Be::SCHEMA_SIZE);
    assert_eq!(
        CorpusCode16Be::access(&code16.0).unwrap().get(),
        CorpusCode16Be::Marker
    );

    let payload = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/2.bin"
    ));
    assert_eq!(payload.0.len(), ExternalCorpusMessage::SCHEMA_SIZE);
    let payload_view = ExternalCorpusMessage::access(&payload.0).unwrap();
    assert_eq!(payload_view.tag(), CorpusTag::Payload);
    assert_eq!(
        payload_view.payload().payload().unwrap().value(),
        0x1122_3344
    );

    let code8 = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/3.bin"
    ));
    assert_eq!(code8.0.len(), CorpusCode8::SCHEMA_SIZE);
    assert_eq!(
        CorpusCode8::access(&code8.0).unwrap().get(),
        CorpusCode8::Max
    );

    let unit = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/4.bin"
    ));
    assert_eq!(unit.0.len(), ExternalCorpusMessage::SCHEMA_SIZE);
    let unit_view = ExternalCorpusMessage::access(&unit.0).unwrap();
    assert_eq!(unit_view.tag(), CorpusTag::Unit);
    assert!(unit_view.payload().unit().is_some());

    let strings = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/5.bin"
    ));
    assert_eq!(strings.0.len(), FuzzAllStrings::SCHEMA_SIZE);
    let _ = FuzzAllStrings::access(&strings.0).unwrap().copy_into();

    let all_features = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/6.bin"
    ));
    assert_eq!(all_features.0.len(), AllFeatures::SCHEMA_SIZE);
    let all_features_view = AllFeatures::access(&all_features.0).unwrap();
    assert_eq!(all_features_view.config().tag(), ConfigKind::Memory);
}

#[test]
fn layout_constants_and_all_features_fixture_are_frozen() {
    assert_eq!(
        (CorpusCode8::SCHEMA_SIZE, CorpusCode8::SCHEMA_ALIGN),
        (1, 1)
    );
    assert_eq!(
        (CorpusCode16Be::SCHEMA_SIZE, CorpusCode16Be::SCHEMA_ALIGN),
        (2, 2)
    );
    assert_eq!(
        (EndianMatrix::SCHEMA_SIZE, EndianMatrix::SCHEMA_ALIGN),
        (8, 4)
    );
    assert_eq!(
        (StringMatrix::SCHEMA_SIZE, StringMatrix::SCHEMA_ALIGN),
        (8, 1)
    );
    assert_eq!(
        (CorpusPayload::SCHEMA_SIZE, CorpusPayload::SCHEMA_ALIGN),
        (4, 4)
    );
    assert_eq!(
        (
            ExternalCorpusMessage::SCHEMA_SIZE,
            ExternalCorpusMessage::SCHEMA_ALIGN
        ),
        (8, 4)
    );
    assert_eq!(
        (AllFeatures::SCHEMA_SIZE, AllFeatures::SCHEMA_ALIGN),
        (112, 16)
    );

    let all_features = producer(include_bytes!(
        "../test-fixtures/schema-corpus/golden/all-features-record.bin"
    ));
    let view = AllFeatures::access(&all_features.0).unwrap();
    assert_eq!(
        view.samples().copy_into(),
        [0x1111_1111, 0x1212_1212, 0x1313_1313]
    );
    assert_eq!(view.config().tag(), ConfigKind::Memory);
    assert_golden(
        &all_features.0,
        include_bytes!("../test-fixtures/schema-corpus/golden/6.bin"),
        "8d8364d711f44feb2183a27db21d7211888c32e3125f15b800b13a42bce207c1",
    );
}

#[test]
fn malformed_access_error_contract_is_frozen() {
    let scalar = producer(&[7]);
    let error = match CorpusCode8::access(&scalar.0) {
        Err(error) => error,
        Ok(_) => panic!("invalid scalar enum bytes must be rejected"),
    };
    assert_eq!(error.kind(), ErrorKind::UnknownEnumValue);
    assert_eq!(error.schema(), "CorpusCode8");

    let unknown_tag = producer(&[CorpusTag::Reserved as u8, 0, 0, 0, 0, 0, 0, 0]);
    let error = ExternalCorpusMessage::access(&unknown_tag.0).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnknownUnionTag);
    assert_eq!(error.schema(), "ExternalCorpusMessage");
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("payload")));
}
