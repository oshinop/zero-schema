use core::mem::{align_of_val, size_of_val};
use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    sequence: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Envelope<'a, T, const N: usize> {
    child: T,
    bytes: &'a [u8; N],
}

type ConcreteEnvelope = Envelope<'static, Child, 4>;

// Type- or const-generic schemas use the runtime storage abstraction once the
// schema is fully concrete. It supplies initialized bytes and the wire alignment.

fn main() {
    let payload = [0x10, 0x20, 0x30, 0x40];
    let value = Envelope {
        child: Child {
            sequence: 0x1122_3344,
        },
        bytes: &payload,
    };

    let mut buffer = zero_schema::make_buffer_for!(ConcreteEnvelope);
    assert_eq!(size_of_val(&buffer), ConcreteEnvelope::WIRE_STRIDE);
    assert_eq!(align_of_val(&buffer), ConcreteEnvelope::WIRE_ALIGN);
    value.encode_into(buffer.as_bytes_mut()).unwrap();

    let decoded: Envelope<'_, Child, 4> = Envelope::parse(buffer.as_bytes()).unwrap();
    assert_eq!(decoded, value);

    // The borrowed field is a zero-copy projection into the aligned buffer.
    let start = buffer.as_bytes().as_ptr() as usize;
    let end = start + buffer.as_bytes().len();
    let projection = decoded.bytes.as_ptr() as usize;
    assert!(projection >= start && projection + decoded.bytes.len() <= end);
}
