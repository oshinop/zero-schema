use zero_schema_macros::zero;
use zs::__private::{SharedInput, WireType};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn main() {
    type Wire = <PublicRoot as WireType>::Wire;
    let bytes = [0_u8];
    let token = __zero_schema_support_publicroot_68f3781aafb1d646::__zero_schema_input_token();
    let _ = SharedInput::<Wire>::from_exact(&bytes, token);
}
