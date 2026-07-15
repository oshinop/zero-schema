use zero_schema_macros::zero;
use zs::__private::{SharedInput, WireType};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn main() {
    type Wire = <PublicRoot as WireType>::Wire;
    let bytes = [0_u8];
    let _ = SharedInput::<Wire>::from_exact(&bytes, ());
}
