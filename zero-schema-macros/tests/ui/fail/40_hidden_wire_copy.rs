use zero_schema_macros::zero;
use zs::__private::WireType;

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn require_copy<T: Copy>() {}

fn main() {
    type Wire = <PublicRoot as WireType>::Wire;
    require_copy::<Wire>();
}
