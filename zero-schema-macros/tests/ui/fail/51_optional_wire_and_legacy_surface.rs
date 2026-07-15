use zero_schema_macros::zero;
use zerocopy::{FromBytes, IntoBytes};
use zs::__private::{WireType, ZeroState};

struct ForgedZero;

impl ZeroState for ForgedZero {
    type Or<Rhs: ZeroState> = Rhs;
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Present = 1,
}

#[zero(crate = zs)]
pub struct Child {
    state: State,
}

#[zero(crate = zs)]
pub struct OptionalRoot {
    maybe: Option<Child>,
}

fn require_into_bytes<T: IntoBytes>(_: &T) {}
fn require_copy<T: Copy>() {}

fn main() {
    type Wire = <OptionalRoot as WireType>::Wire;

    let bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let wire = Wire::ref_from_bytes(&bytes).expect("exact root wire span");
    let _storage = wire.maybe;
    require_into_bytes(wire);
    require_copy::<Wire>();

}
