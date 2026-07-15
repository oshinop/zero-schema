use zero_schema_macros::zero;
use zs::__private::{ProvedExclusive, ProvedShared, WireType, WireTypeSupport};

#[zero(crate = zs)]
struct Opaque {
    enabled: bool,
}

fn main() {
    type Support = <Opaque as WireTypeSupport>::Support;
    type Wire = <Opaque as WireType>::Wire;

    let bytes = [0_u8];
    let shared = zs::__private::SharedInput::<Wire>::from_exact(&bytes, ()).unwrap();
    let _ = ProvedShared::<Support, Wire> {
        input: shared,
        _brand: core::marker::PhantomData,
    };

    let mut bytes = [0_u8];
    let exclusive = zs::__private::ExclusiveInput::<Wire>::from_exact(&mut bytes, ()).unwrap();
    let _ = ProvedExclusive::<Support, Wire> {
        input: exclusive,
        _brand: core::marker::PhantomData,
    };
}
