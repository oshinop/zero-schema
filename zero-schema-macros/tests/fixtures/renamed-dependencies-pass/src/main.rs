#![deny(warnings)]
extern crate zc as zerocopy;


use zero_schema_macros::zero;

#[zero]
struct RenamedDependencies {
    value: u32,
}

const _: usize = core::mem::size_of::<zc::byteorder::U32<zc::byteorder::LittleEndian>>();

fn main() {
    let _ = RenamedDependencies::SCHEMA_SIZE;
    let _ = RenamedDependencies::SCHEMA_ALIGN;
    let _ = RenamedDependencies::SCHEMA_STRIDE;
}
