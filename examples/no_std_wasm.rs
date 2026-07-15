#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_std)]
#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_main)]

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
mod freestanding {
    use core::panic::PanicInfo;

    use zero_schema::zero;

    #[zero]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u8)]
    pub enum PacketKind {
        Empty = 1,
        Data = 2,
    }

    #[zero]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Number {
        #[zero(endian = "little")]
        pub value: u32,
    }

    #[zero]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Payload {
        #[zero(tag = PacketKind::Empty)]
        Empty,
        #[zero(tag = PacketKind::Data)]
        Data(Number),
    }

    #[zero]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Packet {
        pub kind: PacketKind,
        #[zero(tag_field = kind)]
        pub payload: Payload,
    }

    // Reviewed bytes received from a producer: tag, ignored padding, then little-endian u32.
    const REVIEWED_PRODUCER_PACKET: [u8; 8] =
        [PacketKind::Data as u8, 0xa1, 0xb2, 0xc3, 7, 0, 0, 0];

    const _: [(); 8] = [(); Packet::SCHEMA_SIZE];
    const _: [(); 4] = [(); Packet::SCHEMA_ALIGN];
    const _: [(); 8] = [(); Packet::SCHEMA_STRIDE];

    #[repr(C, align(4))]
    struct ProducerPacket {
        bytes: [u8; Packet::SCHEMA_SIZE],
    }

    impl ProducerPacket {
        const fn reviewed() -> Self {
            Self {
                bytes: REVIEWED_PRODUCER_PACKET,
            }
        }
    }

    fn exact_access_succeeds() -> bool {
        let producer = ProducerPacket::reviewed();
        let Ok(view) = Packet::access(&producer.bytes) else {
            return false;
        };
        matches!(
            (view.kind(), view.payload().tag(), view.payload().data()),
            (PacketKind::Data, PacketKind::Data, Some(number)) if number.value() == 7
        )
    }

    // This freestanding example has no console. Linking proves it can receive
    // producer-owned bytes and access one exact, type-valid external-tag record.
    #[unsafe(no_mangle)]
    pub extern "C" fn _start() {
        assert!(exact_access_succeeds());
    }

    #[panic_handler]
    fn panic(_info: &PanicInfo<'_>) -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "none")))]
fn main() {
    // Cargo can build this placeholder on a host; link for `wasm32v1-none` to
    // exercise the freestanding producer-byte access path above.
}
