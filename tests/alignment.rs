use core::mem::{align_of_val, size_of_val};
use zero_schema::{ErrorKind, SchemaError, ZeroSchema};

#[derive(Debug, PartialEq, ZeroSchema)]
struct Direct {
    value: u64,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Borrowed<'a> {
    #[zero(capacity = 8)]
    text: &'a str,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    value: u32,
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum Tag {
    Unit = 1,
    Data = 2,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag)]
enum Message {
    #[zero(tag = Tag::Unit)]
    Unit,
    #[zero(tag = Tag::Data)]
    Data(Child),
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Generic<'a, T, const N: usize> {
    value: T,
    bytes: &'a [u8; N],
}
#[derive(ZeroSchema)]
struct External {
    tag: Tag,
    #[zero(tag_field = tag, align = 8)]
    payload: Message,
}

macro_rules! check_buffer {
    ($schema:ty) => {{
        let buffer = zero_schema::make_buffer_for!($schema);
        assert_eq!(buffer.as_bytes().len(), <$schema>::WIRE_SIZE);
        assert_eq!(align_of_val(&buffer), <$schema>::WIRE_ALIGN);
        assert_eq!(size_of_val(&buffer), <$schema>::WIRE_STRIDE);
        assert_eq!(
            buffer
                .as_bytes()
                .as_ptr()
                .align_offset(<$schema>::WIRE_ALIGN),
            0
        );
    }};
}

#[test]
fn generic_storage_has_exact_layout() {
    check_buffer!(Direct);
    check_buffer!(Borrowed<'static>);
    check_buffer!(Child);
    check_buffer!(Message);
    check_buffer!(External);
    let mut buffer = zero_schema::make_buffer_for!(Generic<'static, Child, 3>);
    assert_eq!(
        buffer.as_bytes().len(),
        Generic::<'static, Child, 3>::WIRE_SIZE
    );
    assert_eq!(
        align_of_val(&buffer),
        Generic::<'static, Child, 3>::WIRE_ALIGN
    );
    assert_eq!(
        size_of_val(&buffer),
        Generic::<'static, Child, 3>::WIRE_STRIDE
    );
    let value = Generic {
        value: Child { value: 8 },
        bytes: &[1, 2, 3],
    };
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(
        Generic::<Child, 3>::parse(buffer.as_bytes()).unwrap(),
        value
    );
}

#[test]
fn plus_one_parse_and_encode_are_misaligned_and_encode_is_untouched() {
    let value = Direct {
        value: 0x1122_3344_5566_7788,
    };
    let mut storage = [0xa5; Direct::WIRE_SIZE + Direct::WIRE_ALIGN + 1];
    let aligned = storage.as_ptr().align_offset(Direct::WIRE_ALIGN);
    let offset = aligned + 1;
    let slice = &mut storage[offset..offset + Direct::WIRE_SIZE];
    let before = slice.to_vec();
    let parse = Direct::parse(slice).unwrap_err();
    assert_eq!(parse.kind(), ErrorKind::Layout);
    assert!(parse.to_string().contains("misaligned address"));
    let encode = value.encode_into(slice).unwrap_err();
    assert_eq!(encode.kind(), ErrorKind::Layout);
    assert!(encode.to_string().contains("misaligned address"));
    assert_eq!(slice, before);
}

#[test]
fn size_errors_precede_alignment_and_leave_destination_untouched() {
    let value = Direct { value: 1 };
    let mut storage = [0x5a; Direct::WIRE_SIZE + Direct::WIRE_ALIGN + 1];
    let aligned = storage.as_ptr().align_offset(Direct::WIRE_ALIGN);
    let offset = aligned + 1;
    let short = &mut storage[offset..offset + Direct::WIRE_SIZE - 1];
    let before = short.to_vec();
    let parse = Direct::parse(short).unwrap_err();
    assert!(parse.to_string().contains("incorrect size"));
    let encode = value.encode_into(short).unwrap_err();
    assert!(encode.to_string().contains("incorrect size"));
    assert_eq!(short, before);
    let prefix = Direct::parse_prefix(short).unwrap_err();
    assert!(prefix.to_string().contains("insufficient bytes"));
}
