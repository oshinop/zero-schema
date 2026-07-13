use core::ffi::CStr;
use core::mem::{align_of, offset_of, size_of, size_of_val};

use widestring::{U16CStr, U16Str};
use zero_schema::{ZeroSchema, ZeroSchemaType};

#[derive(Debug, PartialEq, ZeroSchema)]
struct Details<'a> {
    #[zero(capacity = 12, len_type = u8, tail = "zero")]
    name: &'a str,
    // C string capacities count bytes, including the terminating NUL.
    #[zero(capacity = 8, tail = "zero")]
    label: &'a CStr,
    // Wide-string capacities and tail offsets count u16 code units, not bytes.
    #[zero(capacity = 6, len_type = u8, endian = "native", tail = "zero")]
    category: &'a U16Str,
    #[zero(capacity = 6, endian = "native", tail = "zero")]
    path: &'a U16CStr,
    digest: &'a [u8; 4],
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Record<'a> {
    // The integer's byte order is fixed independently of the host platform.
    #[zero(endian = "big")]
    id: u32,
    active: bool,
    details: Details<'a>,
}
const SUFFIX_LEN: usize = 4;

#[repr(C)]
struct FramedRecord {
    _align: [<Record<'static> as ZeroSchemaType>::Wire; 0],
    bytes: [u8; Record::WIRE_SIZE + SUFFIX_LEN],
}

const _: () = assert!(offset_of!(FramedRecord, bytes) == 0);

fn points_into<T: ?Sized>(value: *const T, bytes: &[u8]) -> bool {
    let address = value.cast::<u8>() as usize;
    let start = bytes.as_ptr() as usize;
    start <= address && address < start + bytes.len()
}

fn main() {
    let category_units = [b'r' as u16, b'u' as u16, b's' as u16, b't' as u16];
    let category = U16Str::from_slice(&category_units);
    let path_units = [b'/' as u16, b't' as u16, b'm' as u16, b'p' as u16, 0];
    let path = U16CStr::from_slice(&path_units).expect("path has one trailing NUL");
    let digest = [0x10, 0x20, 0x30, 0x40];
    let original = Record {
        id: 0x0102_0304,
        active: true,
        details: Details {
            name: "config",
            label: c"stable",
            category,
            path,
            digest: &digest,
        },
    };

    // `encode` returns initialized, correctly aligned wire storage. Keep it alive
    // for at least as long as any borrowed values returned by `parse`.
    let buffer = original
        .encode()
        .expect("the value fits every declared capacity");

    assert_eq!(
        Record::WIRE_SIZE,
        size_of::<<Record<'static> as ZeroSchemaType>::Wire>()
    );
    assert_eq!(
        Record::WIRE_ALIGN,
        align_of::<<Record<'static> as ZeroSchemaType>::Wire>()
    );
    assert_eq!(Record::WIRE_STRIDE, size_of_val(&buffer));
    assert_eq!(Record::LAYOUT.size(), Record::WIRE_SIZE);
    assert_eq!(Record::LAYOUT.align(), Record::WIRE_ALIGN);
    assert_eq!(Record::LAYOUT.stride(), Record::WIRE_STRIDE);

    let decoded = Record::parse(buffer.as_bytes()).expect("encoded bytes parse exactly");
    assert_eq!(decoded, original);

    // Every borrowed projection points into the encoded buffer: parsing did not copy it.
    let encoded = buffer.as_bytes();
    assert!(points_into(decoded.details.name.as_ptr(), encoded));
    assert!(points_into(decoded.details.label.as_ptr(), encoded));
    assert!(points_into(decoded.details.category.as_ptr(), encoded));
    assert!(points_into(decoded.details.path.as_ptr(), encoded));
    assert!(points_into(decoded.details.digest.as_ptr(), encoded));

    let suffix = [0xde, 0xad, 0xbe, 0xef];
    // `parse_prefix` still requires aligned input. The zero-length wire array
    // aligns the larger frame without unsafe code or assumptions about `Vec`.
    let mut framed = FramedRecord {
        _align: [],
        bytes: [0; Record::WIRE_SIZE + SUFFIX_LEN],
    };
    framed.bytes[..Record::WIRE_SIZE].copy_from_slice(encoded);
    framed.bytes[Record::WIRE_SIZE..].copy_from_slice(&suffix);
    let (from_prefix, remainder) =
        Record::parse_prefix(&framed.bytes).expect("wire prefix is complete");
    assert_eq!(from_prefix, original);
    assert_eq!(remainder, suffix);
    assert_eq!(
        remainder.as_ptr(),
        framed.bytes[Record::WIRE_SIZE..].as_ptr()
    );

    println!(
        "record id={:08x} active={} name={} category_units={} suffix_bytes={}",
        decoded.id,
        decoded.active,
        decoded.details.name,
        decoded.details.category.len(),
        remainder.len()
    );
}
