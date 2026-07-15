use zero_schema_macros::zero;
use zs::__private::{ExclusiveInput, WireType};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn main() {
    type Wire = <PublicRoot as WireType>::Wire;
    let mut bytes = [0_u8];
    let _ = ExclusiveInput::<Wire>::from_exact(&mut bytes, ());
}
