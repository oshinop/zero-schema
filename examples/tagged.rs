use zero_schema::{ErrorKind, ErrorPathSegment, SchemaError, ZeroSchema};

// Scalar tags have a closed domain: any wire value not listed here is rejected.
#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum PacketTag {
    Empty = 1,
    Reading = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Reading {
    sequence: u32,
    value: i16,
}

// Without `tag_field`, the tag is stored inside the union's own wire layout.
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = PacketTag, tail = "zero")]
enum Packet {
    #[zero(tag = PacketTag::Empty)]
    Empty,
    #[zero(tag = PacketTag::Reading)]
    Reading(Reading),
}

// Here the same closed tag domain is stored by the containing record. The
// payload refers to that sibling field instead of writing a second tag.
#[derive(Debug, PartialEq, ZeroSchema)]
struct Envelope {
    tag: PacketTag,
    #[zero(tag_field = tag)]
    packet: Packet,
}

fn main() {
    let empty = Packet::Empty;
    let empty_buffer = empty.encode().unwrap();
    assert_eq!(Packet::parse(empty_buffer.as_bytes()).unwrap(), empty);

    let reading = Packet::Reading(Reading {
        sequence: 0x0102_0304,
        value: -25,
    });
    let packet_buffer = reading.encode().unwrap();
    assert_eq!(Packet::parse(packet_buffer.as_bytes()).unwrap(), reading);

    let envelope = Envelope {
        tag: PacketTag::Reading,
        packet: Packet::Reading(Reading {
            sequence: 17,
            value: 42,
        }),
    };
    let mut envelope_buffer = envelope.encode().unwrap();
    assert_eq!(
        Envelope::parse(envelope_buffer.as_bytes()).unwrap(),
        envelope
    );

    let mismatch = Envelope {
        tag: PacketTag::Empty,
        packet: Packet::Reading(Reading {
            sequence: 17,
            value: 42,
        }),
    };
    let error = mismatch
        .encode_into(envelope_buffer.as_bytes_mut())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TagMismatch);
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("packet")));
    assert_eq!(
        error.to_string(),
        "Envelope.packet: external tag 1 does not match selected tag 2"
    );
}
