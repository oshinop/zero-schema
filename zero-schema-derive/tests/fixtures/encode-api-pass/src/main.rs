use zero_schema_derive::ZeroSchema;

#[derive(ZeroSchema)]
#[zero(crate = zs, endian = "little")]
#[repr(u16)]
enum Tag {
    Data = 0x1234,
}

#[derive(ZeroSchema)]
#[zero(crate = zs)]
struct Record<'a> {
    bytes: &'a [u8; 2],
    value: u16,
}

#[derive(ZeroSchema)]
#[zero(crate = zs, tag = Tag)]
enum Message<'a> {
    #[zero(tag = Tag::Data)]
    Data(Record<'a>),
}

fn assert_storage<W, const N: usize>(bytes: &zs::AlignedBytes<W, N>, expected: &[u8], align: usize) {
    assert_eq!(bytes.as_bytes(), expected);
    assert_eq!(bytes.as_bytes().len(), N);
    assert_eq!((bytes.as_bytes().as_ptr() as usize) % align, 0);
}

fn main() {
    let tag = Tag::Data.encode().unwrap();
    assert_storage(&tag, &[0x34, 0x12], Tag::WIRE_ALIGN);

    let record = Record { bytes: &[0xaa, 0xbb], value: 0x1234 };
    let encoded = record.encode().unwrap();
    assert_storage(&encoded, &[0xaa, 0xbb, 0x34, 0x12], Record::WIRE_ALIGN);
    let encoded = {
        let local = [0xaa, 0xbb];
        let message = Message::Data(Record { bytes: &local, value: 0x1234 });
        message.encode().unwrap()
    };
    assert_eq!(&encoded.as_bytes()[..2], &[0x34, 0x12]);
    assert_eq!(&encoded.as_bytes()[2..], &[0xaa, 0xbb, 0x34, 0x12]);
}
