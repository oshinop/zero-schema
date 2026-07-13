#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_std)]
#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_main)]

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
mod freestanding {
    use core::panic::PanicInfo;
    use zero_schema::ZeroSchema;

    #[derive(ZeroSchema)]
    struct Greeting<'a> {
        sequence: u16,
        #[zero(capacity = 12, len_type = u8)]
        text: &'a str,
    }

    fn roundtrip() -> bool {
        let original = Greeting {
            sequence: 7,
            text: "hello wasm",
        };
        let buffer = match original.encode() {
            Ok(buffer) => buffer,
            Err(_) => return false,
        };

        let decoded = match Greeting::parse(buffer.as_bytes()) {
            Ok(value) => value,
            Err(_) => return false,
        };
        if decoded.sequence != original.sequence || decoded.text != original.text {
            return false;
        }

        matches!(
            Greeting::parse_prefix(buffer.as_bytes()),
            Ok((value, rest))
                if value.sequence == original.sequence
                    && value.text == original.text
                    && rest.is_empty()
        )
    }

    // This example has no console. Successfully linking the freestanding target is
    // the proof: it exercises derive, allocation-free encoding, and borrowed decoding.
    #[unsafe(no_mangle)]
    pub extern "C" fn _start() {
        assert!(roundtrip());
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
    // Cargo can discover and build this example on a host; the meaningful check is
    // linking it for wasm32v1-none with default features disabled.
}
