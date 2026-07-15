use zero_schema_macros::zero;
use zs::__private::{InputAccess, SharedInput, WireType};

struct Attacker;
impl InputAccess for Attacker {
    type Token = ();
}

#[zero(crate = zs)]
struct Opaque {
    value: u8,
}

#[zero(crate = zs)]
struct Generic<const N: usize> {
    values: [u8; N],
}

fn main() {
    let _: Wire;
    let _ = Opaque::Wire;
    let _ = Opaque::parse(&[1_u8]);
    let _ = Opaque::parse_prefix(&[1_u8]);
    let _ = Opaque::encode(&Opaque { value: 1 });
    let _ = Opaque::encode_into(&Opaque { value: 1 }, &mut [1_u8]);
    let _ = Opaque::build();
    let _ = Opaque::init();
    let _ = GenericSchemaBuffer::new();
    let bytes = [1_u8];
    type OpaqueWire = <Opaque as WireType>::Wire;
    let attacker_input = SharedInput::<OpaqueWire>::from_exact(&bytes, ()).unwrap();
    let _ = attacker_input.wire::<Attacker>(());
    let _ = attacker_input.read_copy::<OpaqueWire>(0);
    let view = Opaque::access(&bytes).unwrap();
    let _ = view.to_owned();
    let _ = OpaqueRef::from_view(&view);
    let _ = view.copy_view();
    let _ = view.raw();
    let _ = view.wire();
    let _ = view.input;
    let mut mutable_bytes = [1_u8];
    let mut view = Opaque::access_mut(&mut mutable_bytes).unwrap();
    let _ = view.to_owned();
    let _ = OpaqueMut::from_view(&mut view);
    let _ = view.copy_view();
    let _ = view.raw();
    let _ = view.wire();
    let _ = view.input;
}
