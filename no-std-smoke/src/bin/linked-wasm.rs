#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_std)]
#![cfg_attr(all(target_arch = "wasm32", target_os = "none"), no_main)]

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() {
    let _ = zero_schema_no_std_smoke::smoke_access_read_mutate_copy();
}

#[cfg(not(all(target_arch = "wasm32", target_os = "none")))]
fn main() {}
