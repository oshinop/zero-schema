use core::{
    ffi::CStr,
    mem::{align_of, size_of},
};

use widestring::{U16CStr, U16Str};
use zero_schema::{ErrorKind, SchemaError, zero};

#[zero(align = 8)]
struct BorrowedForms<'a> {
    #[zero(capacity = 5, len_type = u8)]
    text: &'a str,
    #[zero(capacity = 5)]
    c_text: &'a CStr,
    #[zero(capacity = 3, len_type = u8, align = 4)]
    wide_text: &'a U16Str,
    #[zero(capacity = 3)]
    wide_c_text: &'a U16CStr,
    bytes: &'a [u8; 4],
    #[zero(endian = "big")]
    sequence: u32,
}

// These offsets and bytes were reviewed as producer output. The example never
// encodes a value or derives this fixture through generated layout metadata.
const TEXT_OFFSET: usize = 0;
const TEXT_DATA_OFFSET: usize = 1;
const C_TEXT_OFFSET: usize = 6;
const WIDE_TEXT_OFFSET: usize = 12;
const WIDE_TEXT_DATA_IN_FIELD: usize = 2;
const WIDE_C_TEXT_OFFSET: usize = 20;
const BYTES_OFFSET: usize = 26;
const SEQUENCE_OFFSET: usize = 32;

// The u16 string storage is native-endian. Both arrays are independently
// reviewed producer output for their corresponding target endian.
#[cfg(target_endian = "little")]
const REVIEWED_PRODUCER_RECORD: [u8; 40] = [
    3, b'i', b'c', b'e', 0xa1, 0xa2, // text (length, active bytes, unused capacity)
    b'h', b'i', 0, 0xb1, 0xb2, // C string (terminator, then unused capacity)
    0xd0, // record padding
    2, 0xd1, 0x34, 0x12, 0x78, 0x56, 0xef, 0xbe, // wide text and its unused unit
    0x21, 0x43, 0, 0, 0xef, 0xcd, // wide C string and its unused unit
    0x10, 0x20, 0x30, 0x40, // fixed bytes
    0xe1, 0xe2, // record padding
    1, 2, 3, 4, // big-endian sequence
    0xf1, 0xf2, 0xf3, 0xf4, // tail padding
];

#[cfg(target_endian = "big")]
const REVIEWED_PRODUCER_RECORD: [u8; 40] = [
    3, b'i', b'c', b'e', 0xa1, 0xa2, // text (length, active bytes, unused capacity)
    b'h', b'i', 0, 0xb1, 0xb2, // C string (terminator, then unused capacity)
    0xd0, // record padding
    2, 0xd1, 0x12, 0x34, 0x56, 0x78, 0xbe, 0xef, // wide text and its unused unit
    0x43, 0x21, 0, 0, 0xcd, 0xef, // wide C string and its unused unit
    0x10, 0x20, 0x30, 0x40, // fixed bytes
    0xe1, 0xe2, // record padding
    1, 2, 3, 4, // big-endian sequence
    0xf1, 0xf2, 0xf3, 0xf4, // tail padding
];

#[cfg(target_endian = "little")]
const SUCCESSFUL_MUTATION_BYTES: [u8; 40] = [
    2, b'o', b'x', b'e', 0xa1, 0xa2, // shorter text leaves inactive capacity unchanged
    b'Q', 0, 0, 0xb1, 0xb2, // shorter C string leaves inactive capacity unchanged
    0xd0, 1, 0xd1, 0x0b, 0x0a, 0x78, 0x56, 0xef, 0xbe, 0x0d, 0x0c, 0, 0, 0xef,
    0xcd, // inactive wide capacity stays unchanged
    b'R', b'U', b'S', b'T', 0xe1, 0xe2, 0x0a, 0x0b, 0x0c, 0x0d, 0xf1, 0xf2, 0xf3, 0xf4,
];

#[cfg(target_endian = "big")]
const SUCCESSFUL_MUTATION_BYTES: [u8; 40] = [
    2, b'o', b'x', b'e', 0xa1, 0xa2, // shorter text leaves inactive capacity unchanged
    b'Q', 0, 0, 0xb1, 0xb2, // shorter C string leaves inactive capacity unchanged
    0xd0, 1, 0xd1, 0x0a, 0x0b, 0x56, 0x78, 0xbe, 0xef, 0x0c, 0x0d, 0, 0, 0xcd,
    0xef, // inactive wide capacity stays unchanged
    b'R', b'U', b'S', b'T', 0xe1, 0xe2, 0x0a, 0x0b, 0x0c, 0x0d, 0xf1, 0xf2, 0xf3, 0xf4,
];

const _: [(); 40] = [(); BorrowedForms::SCHEMA_SIZE];
const _: [(); 8] = [(); BorrowedForms::SCHEMA_ALIGN];
const _: [(); 40] = [(); BorrowedForms::SCHEMA_STRIDE];

#[repr(C, align(8))]
struct ProducerRecord {
    bytes: [u8; BorrowedForms::SCHEMA_SIZE],
}

const _: [(); BorrowedForms::SCHEMA_SIZE] = [(); size_of::<ProducerRecord>()];
const _: [(); BorrowedForms::SCHEMA_ALIGN] = [(); align_of::<ProducerRecord>()];

impl ProducerRecord {
    const fn reviewed() -> Self {
        Self {
            bytes: REVIEWED_PRODUCER_RECORD,
        }
    }
}

fn assert_fixture_pointer(pointer: usize, length: usize, start: usize, end: usize) {
    assert!(
        pointer >= start && pointer + length <= end,
        "borrowed getter must remain within producer storage"
    );
}

fn main() {
    let producer = ProducerRecord::reviewed();
    assert_eq!(size_of::<ProducerRecord>(), BorrowedForms::SCHEMA_SIZE);
    assert_eq!(align_of::<ProducerRecord>(), BorrowedForms::SCHEMA_ALIGN);
    assert_eq!(producer.bytes, REVIEWED_PRODUCER_RECORD);

    // Access eagerly validates every field, while these getters borrow the
    // active data only: trailing capacity and record padding are ignored.
    let view = BorrowedForms::access(&producer.bytes).expect("reviewed producer bytes are valid");
    assert_eq!(view.text(), "ice");
    assert_eq!(view.c_text().to_bytes(), b"hi");
    assert_eq!(view.wide_text().as_slice(), &[0x1234, 0x5678]);
    assert_eq!(view.wide_c_text().as_slice(), &[0x4321]);
    assert_eq!(view.bytes(), &[0x10, 0x20, 0x30, 0x40]);
    assert_eq!(view.sequence(), 0x0102_0304);
    assert_eq!(
        &producer.bytes[SEQUENCE_OFFSET..SEQUENCE_OFFSET + size_of::<u32>()],
        &[1, 2, 3, 4],
        "the explicit big-endian scalar is producer-stable"
    );

    let start = producer.bytes.as_ptr() as usize;
    let end = start + producer.bytes.len();
    assert_fixture_pointer(view.text().as_ptr() as usize, view.text().len(), start, end);
    assert_fixture_pointer(
        view.c_text().as_ptr() as usize,
        view.c_text().to_bytes_with_nul().len(),
        start,
        end,
    );
    assert_fixture_pointer(
        view.wide_text().as_ptr() as usize,
        size_of::<u16>() * view.wide_text().len(),
        start,
        end,
    );
    assert_fixture_pointer(
        view.wide_c_text().as_ptr() as usize,
        core::mem::size_of_val(view.wide_c_text().as_slice_with_nul()),
        start,
        end,
    );
    assert_fixture_pointer(
        view.bytes().as_ptr() as usize,
        view.bytes().len(),
        start,
        end,
    );
    assert_eq!(
        view.text().as_ptr() as usize - start,
        TEXT_OFFSET + TEXT_DATA_OFFSET
    );
    assert_eq!(view.c_text().as_ptr() as usize - start, C_TEXT_OFFSET);
    assert_eq!(
        view.wide_text().as_ptr() as usize - start,
        WIDE_TEXT_OFFSET + WIDE_TEXT_DATA_IN_FIELD
    );
    assert_eq!(
        view.wide_c_text().as_ptr() as usize - start,
        WIDE_C_TEXT_OFFSET
    );
    assert_eq!(view.bytes().as_ptr() as usize - start, BYTES_OFFSET);

    // `copy_into` is the explicit opt-in materialization; it keeps the same
    // borrowed forms rather than allocating owned strings.
    let copied = view.copy_into();
    assert_eq!(copied.text, "ice");
    assert_eq!(copied.c_text.to_bytes(), b"hi");
    assert_eq!(copied.wide_text.as_slice(), &[0x1234, 0x5678]);
    assert_eq!(copied.wide_c_text.as_slice(), &[0x4321]);
    assert_eq!(copied.bytes, &[0x10, 0x20, 0x30, 0x40]);

    let mut producer = producer;
    {
        let mut view = BorrowedForms::access_mut(&mut producer.bytes)
            .expect("reviewed producer bytes remain valid for mutation");
        view.text_mut().set("ox").expect("within text capacity");
        view.c_text_mut()
            .set(c"Q")
            .expect("within C string capacity");
        view.wide_text_mut()
            .set(U16Str::from_slice(&[0x0a0b]))
            .expect("within wide string capacity");
        view.wide_c_text_mut()
            .set(U16CStr::from_slice(&[0x0c0d, 0]).expect("single terminator"))
            .expect("within wide C string capacity");
        view.bytes_mut()
            .set(b"RUST")
            .expect("fixed byte array has an exact source length");
        view.sequence_mut()
            .set(0x0a0b_0c0d)
            .expect("scalar mutation is representable");
    }
    assert_eq!(producer.bytes, SUCCESSFUL_MUTATION_BYTES);
    assert_eq!(
        &producer.bytes[SEQUENCE_OFFSET..SEQUENCE_OFFSET + size_of::<u32>()],
        &[0x0a, 0x0b, 0x0c, 0x0d]
    );

    // Fresh access proves the changed active values and confirms that all
    // inactive capacity and padding remained exactly as supplied by producer.
    let refreshed =
        BorrowedForms::access(&producer.bytes).expect("successful mutation stays valid");
    assert_eq!(refreshed.text(), "ox");
    assert_eq!(refreshed.c_text().to_bytes(), b"Q");
    assert_eq!(refreshed.wide_text().as_slice(), &[0x0a0b]);
    assert_eq!(refreshed.wide_c_text().as_slice(), &[0x0c0d]);
    assert_eq!(refreshed.bytes(), b"RUST");
    assert_eq!(refreshed.sequence(), 0x0a0b_0c0d);

    let mut rejected = ProducerRecord::reviewed();
    let before = rejected.bytes;
    {
        let mut view = BorrowedForms::access_mut(&mut rejected.bytes)
            .expect("reviewed producer bytes remain valid for mutation");
        let error = view
            .text_mut()
            .set("too-long")
            .expect_err("text source exceeds its five-byte capacity");
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    }
    assert_eq!(
        rejected.bytes, before,
        "failed mutation must preserve every producer byte exactly"
    );

    println!(
        "borrowed strings: text={} c_text={:?} wide_units={} fixed_bytes={}",
        BorrowedForms::access(&producer.bytes).unwrap().text(),
        BorrowedForms::access(&producer.bytes)
            .unwrap()
            .c_text()
            .to_bytes(),
        BorrowedForms::access(&producer.bytes)
            .unwrap()
            .wide_text()
            .len(),
        BorrowedForms::access(&producer.bytes)
            .unwrap()
            .bytes()
            .len(),
    );
}
