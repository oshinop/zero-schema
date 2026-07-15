use zero_schema_macros::zero;
use zs::__private::{RootInputAccess, WireType};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

type Wire = <PublicRoot as WireType>::Wire;

impl RootInputAccess for Wire {
    type Token = ();
}

fn main() {}
