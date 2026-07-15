use core::mem::{align_of, size_of};

use zero_schema::{ErrorKind, SchemaError, zero};

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PacketKind {
    Reading = 1,
    Control = 2,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Reading {
    pub sequence: u8,
    pub value: u8,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Control {
    pub level: u8,
    pub enabled: bool,
}

// A tagged payload is not a root: its unique external tag lives in `Envelope`.
#[zero]
#[derive(Debug, PartialEq)]
pub enum Packet {
    #[zero(tag = PacketKind::Reading)]
    Reading(Reading),
    #[zero(tag = PacketKind::Control)]
    Control(Control),
}

#[zero(align = 4)]
#[derive(Debug, PartialEq)]
pub struct Envelope {
    pub kind: PacketKind,
    #[zero(tag_field = kind)]
    pub packet: Packet,
}

// Reviewed producer output: tag `Reading`, then its selected payload, then padding.
const REVIEWED_PRODUCER_ENVELOPE: [u8; 4] = [PacketKind::Reading as u8, 17, 42, 0xa5];

const _: [(); 4] = [(); Envelope::SCHEMA_SIZE];
const _: [(); 4] = [(); Envelope::SCHEMA_ALIGN];

#[repr(C, align(4))]
struct ProducerEnvelope {
    bytes: [u8; Envelope::SCHEMA_SIZE],
}

impl ProducerEnvelope {
    const fn reviewed() -> Self {
        Self {
            bytes: REVIEWED_PRODUCER_ENVELOPE,
        }
    }
}

fn main() {
    let mut producer = ProducerEnvelope::reviewed();
    assert_eq!(size_of::<ProducerEnvelope>(), Envelope::SCHEMA_SIZE);
    assert_eq!(align_of::<ProducerEnvelope>(), Envelope::SCHEMA_ALIGN);

    let view = Envelope::access(&producer.bytes).expect("reviewed producer bytes are valid");
    assert_eq!(view.kind(), PacketKind::Reading);
    assert_eq!(view.packet().tag(), PacketKind::Reading);
    let reading = view
        .packet()
        .reading()
        .expect("the external tag selects Reading");
    assert_eq!((reading.sequence(), reading.value()), (17, 42));

    // The selected variant materializes only after tag-coupled selection.
    assert_eq!(
        reading.copy_into(),
        Reading {
            sequence: 17,
            value: 42,
        }
    );
    assert!(view.packet().control().is_none());

    {
        let mut envelope = Envelope::access_mut(&mut producer.bytes).unwrap();
        let mut packet = envelope.packet_mut();
        packet
            .reading_mut()
            .expect("the selected payload can be constrained-mutated")
            .value_mut()
            .set(99)
            .unwrap();
    }

    let reading = Envelope::access(&producer.bytes)
        .unwrap()
        .packet()
        .reading()
        .expect("the selected payload remains Reading after a field mutation");
    assert_eq!(reading.value(), 99);

    let before_failed_patch = producer.bytes;
    let error = Envelope::access_mut(&mut producer.bytes)
        .unwrap()
        .copy_from(&EnvelopePatch {
            kind: Some(PacketKind::Control),
            ..Default::default()
        })
        .expect_err("the external tag cannot be patched without its selected payload");
    assert_eq!(error.kind(), ErrorKind::TagOnlyPatch);
    assert_eq!(producer.bytes, before_failed_patch);

    // The complete payload patch derives its coupled sibling tag. A successful
    // switch commits the validated payload before publishing that tag, preserving
    // an accessible selected payload without exposing raw layout mutation.
    Envelope::access_mut(&mut producer.bytes)
        .unwrap()
        .copy_from(&EnvelopePatch {
            packet: Some(PacketPatch::from(Packet::Control(Control {
                level: 5,
                enabled: true,
            }))),
            ..Default::default()
        })
        .expect("a complete switch is atomic and type-valid");

    let view = Envelope::access(&producer.bytes).unwrap();
    let control = view
        .packet()
        .control()
        .expect("the complete patch switched variants");
    assert_eq!(
        (view.kind(), view.packet().tag()),
        (PacketKind::Control, PacketKind::Control)
    );
    assert_eq!((control.level(), control.enabled()), (5, true));
    println!(
        "tagged kind={:?} control.level={}",
        view.packet().tag(),
        control.level()
    );
}
