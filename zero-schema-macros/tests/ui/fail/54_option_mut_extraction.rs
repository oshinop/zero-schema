#![deny(warnings)]

use zero_schema_macros::zero;
use zs::__private::WireTypeSupport;

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

type RootSupport = <OptionalRoot as WireTypeSupport>::Support;

fn main() {
    let mut storage = zs::make_schema_buffer!(OptionalRoot);
    let mut root = OptionalRoot::access_mut(storage.as_bytes_mut()).expect("all-zero optional is valid");
    let zs::OptionMut { mut input, token, .. } = root.maybe_mut();
    input.clear_all::<RootSupport>(token);
}
