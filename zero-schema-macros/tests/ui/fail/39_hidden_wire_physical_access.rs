use zero_schema_macros::zero;
use zerocopy::{FromBytes, IntoBytes};
use zs::__private::WireType;

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn require_into_bytes<T: IntoBytes>(_: &T) {}

fn main() {
    type Wire = <PublicRoot as WireType>::Wire;
    let bytes = [0_u8];
    let wire = Wire::ref_from_bytes(&bytes).expect("exactly one wire");
    let _physical = wire.value;
    require_into_bytes(wire);
    let second_wire = Wire::ref_from_bytes(&bytes).expect("exactly one wire");
    let _copied: Wire = *second_wire;
}
