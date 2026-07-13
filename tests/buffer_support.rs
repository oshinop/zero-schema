use core::mem::{align_of_val, size_of_val};
use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
enum ScalarCode {
    Value = 0x1234,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Direct {
    number: u32,
    flag: bool,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    value: u16,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Nested {
    prefix: u8,
    child: Child,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Borrowed<'a> {
    #[zero(capacity = 8)]
    text: &'a str,
    bytes: &'a [u8; 3],
}

#[derive(ZeroSchema)]
#[repr(u8)]
enum MessageTag {
    Empty = 1,
    Data = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = MessageTag)]
enum Message {
    #[zero(tag = MessageTag::Empty)]
    Empty,
    #[zero(tag = MessageTag::Data)]
    Data(Child),
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Concrete {
    marker: u16,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Envelope<'a, T, const N: usize> {
    #[zero(capacity = 8)]
    name: &'a str,
    value: T,
    bytes: &'a [u8; N],
}

macro_rules! assert_storage {
    ($schema:ty) => {{
        let mut buffer = zero_schema::make_buffer_for!($schema);
        assert_eq!(align_of_val(&buffer), <$schema>::WIRE_ALIGN);
        assert_eq!(size_of_val(&buffer), <$schema>::WIRE_STRIDE);
        assert_eq!(buffer.as_bytes(), &[0; <$schema>::WIRE_SIZE]);
        buffer.as_bytes_mut().fill(0xa5);
        assert_eq!(buffer.as_bytes(), &[0xa5; <$schema>::WIRE_SIZE]);
        buffer
    }};
}

#[test]
fn generated_buffers_have_exact_safe_storage_layout_and_roundtrip() {
    let mut scalar = assert_storage!(ScalarCode);
    ScalarCode::Value
        .encode_into(scalar.as_bytes_mut())
        .unwrap();
    assert_eq!(
        ScalarCode::parse(scalar.as_bytes()).unwrap(),
        ScalarCode::Value
    );

    let mut direct = assert_storage!(Direct);
    let direct_value = Direct {
        number: 0x0102_0304,
        flag: true,
    };
    direct_value.encode_into(direct.as_bytes_mut()).unwrap();
    assert_eq!(Direct::parse(direct.as_bytes()).unwrap(), direct_value);

    let mut nested = assert_storage!(Nested);
    let nested_value = Nested {
        prefix: 7,
        child: Child { value: 0x3344 },
    };
    nested_value.encode_into(nested.as_bytes_mut()).unwrap();
    assert_eq!(Nested::parse(nested.as_bytes()).unwrap(), nested_value);

    let mut borrowed = assert_storage!(Borrowed<'static>);
    let borrowed_value = Borrowed {
        text: "hello",
        bytes: &[4, 5, 6],
    };
    borrowed_value.encode_into(borrowed.as_bytes_mut()).unwrap();
    assert_eq!(
        Borrowed::parse(borrowed.as_bytes()).unwrap(),
        borrowed_value
    );

    let mut message = assert_storage!(Message);
    let message_value = Message::Data(Child { value: 0xabcd });
    message_value.encode_into(message.as_bytes_mut()).unwrap();
    assert_eq!(Message::parse(message.as_bytes()).unwrap(), message_value);
}

#[test]
fn concrete_generic_buffer_is_safe_aligned_storage_and_borrows_from_it() {
    let mut buffer = zero_schema::make_buffer_for!(Envelope<'static, Concrete, 3>);
    assert_eq!(
        buffer.as_bytes(),
        &[0; Envelope::<'static, Concrete, 3>::WIRE_SIZE]
    );
    buffer.as_bytes_mut().fill(0x5a);
    assert_eq!(
        buffer.as_bytes(),
        &[0x5a; Envelope::<'static, Concrete, 3>::WIRE_SIZE]
    );

    let original = Envelope {
        name: "schema",
        value: Concrete { marker: 0x2468 },
        bytes: &[9, 8, 7],
    };
    original.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded = Envelope::<'_, Concrete, 3>::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded, original);

    let start = buffer.as_bytes().as_ptr() as usize;
    let end = start + buffer.as_bytes().len();
    let name = decoded.name.as_ptr() as usize;
    let fixed = decoded.bytes.as_ptr() as usize;
    assert!(start <= name && name + decoded.name.len() <= end);
    assert!(start <= fixed && fixed + decoded.bytes.len() <= end);
}
