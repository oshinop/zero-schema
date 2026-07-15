#![no_std]

use zero_schema::zero;

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SmokeKind {
    Empty = 1,
    Data = 2,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Number {
    value: u32,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Payload {
    #[zero(tag = SmokeKind::Empty)]
    Empty,
    #[zero(tag = SmokeKind::Data)]
    Data(Number),
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet {
    kind: SmokeKind,
    maybe_kind: Option<SmokeKind>,
    #[zero(tag_field = kind)]
    payload: Payload,
}

const PRODUCER_PACKET: [u8; 8] = [SmokeKind::Data as u8, 0, 0, 0, 7, 0, 0, 0];

const _: [(); 8] = [(); Packet::SCHEMA_SIZE];
const _: [(); 4] = [(); Packet::SCHEMA_ALIGN];
const _: [(); 8] = [(); Packet::SCHEMA_STRIDE];

#[repr(C, align(4))]
struct ProducerPacket {
    bytes: [u8; Packet::SCHEMA_SIZE],
}

impl ProducerPacket {
    const fn from_reviewed_bytes() -> Self {
        Self {
            bytes: PRODUCER_PACKET,
        }
    }
}

/// Exercises a producer-owned external-tag record and a zero-sentinel option
/// without initializing wire bytes in Rust or enabling allocation.
pub fn smoke_access_read_mutate_copy() -> u32 {
    let mut producer = ProducerPacket::from_reviewed_bytes();

    let view = match Packet::access(&producer.bytes) {
        Ok(view) => view,
        Err(_) => return 1,
    };
    if view.kind() != SmokeKind::Data {
        return 2;
    }
    if view.maybe_kind().is_some() {
        return 3;
    }
    let payload = view.payload();
    if payload.tag() != SmokeKind::Data {
        return 4;
    }
    match payload.data() {
        Some(number) if number.value() == 7 => {}
        _ => return 5,
    }
    match view.copy_into() {
        Packet {
            maybe_kind: None,
            payload: Payload::Data(Number { value: 7 }),
            ..
        } => {}
        _ => return 6,
    }

    {
        let mut view = match Packet::access_mut(&mut producer.bytes) {
            Ok(view) => view,
            Err(_) => return 7,
        };
        if view.maybe_kind_mut().set(Some(SmokeKind::Empty)).is_err() {
            return 8;
        }
    }
    match Packet::access(&producer.bytes) {
        Ok(view) if view.maybe_kind() == Some(SmokeKind::Empty) => {}
        _ => return 9,
    }

    {
        let mut view = match Packet::access_mut(&mut producer.bytes) {
            Ok(view) => view,
            Err(_) => return 10,
        };
        if view.maybe_kind_mut().set(None).is_err() {
            return 11;
        }
        let mut payload = view.payload_mut();
        let Some(mut number) = payload.data_mut() else {
            return 12;
        };
        if number.value_mut().set(9).is_err() {
            return 13;
        }
    }

    let view = match Packet::access(&producer.bytes) {
        Ok(view) => view,
        Err(_) => return 14,
    };
    match (view.maybe_kind(), view.payload().data()) {
        (None, Some(number)) if number.value() == 9 => 0,
        _ => 15,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_producer_record_supports_capabilities() {
        assert_eq!(smoke_access_read_mutate_copy(), 0);
    }
}
