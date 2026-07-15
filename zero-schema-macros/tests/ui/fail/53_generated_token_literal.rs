#![deny(warnings)]

use zero_schema_macros::zero;
use zs::__private::{InputAccess, SharedInput, WireType, WireTypeSupport};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

type Wire = <PublicRoot as WireType>::Wire;
type Support = <PublicRoot as WireTypeSupport>::Support;
type Token = <Support as InputAccess>::Token;

fn main() {
    let bytes = [0_u8; PublicRoot::SCHEMA_SIZE];
    let token = Token { _private: () };
    let _ = SharedInput::<Wire>::from_exact(&bytes, token);
}
