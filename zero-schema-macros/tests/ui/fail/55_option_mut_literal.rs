#![deny(warnings)]

use zero_schema_macros::zero;

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

type Adapter = __zero_schema_support_optionalroot_5161d5ec2fc1a37b::MaybeOptionAdapter;

fn fake<T>() -> T {
    panic!("compile-time privacy probe")
}

fn main() {
    let _: zs::OptionMut<'static, Child, Adapter> = zs::OptionMut {
        input: fake(),
        token: fake(),
        _adapter: core::marker::PhantomData,
    };
}
