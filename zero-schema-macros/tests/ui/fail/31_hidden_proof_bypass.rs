use zero_schema_macros::zero;
use zs::__private::{
    LogicalSchema, ProvedShared, SchemaSupport, SharedInput, WireType, WireTypeSupport,
};

#[zero(crate = zs)]
struct Opaque {
    enabled: bool,
}

fn main() {
    type Support = <Opaque as WireTypeSupport>::Support;
    type Wire = <Opaque as WireType>::Wire;

    let bytes = [0_u8];
    let make_ref_input = SharedInput::<Wire>::from_exact(&bytes, ()).unwrap();
    let _ = <Support as SchemaSupport>::make_ref(make_ref_input);

    let literal_input = SharedInput::<Wire>::from_exact(&bytes, ()).unwrap();
    let _ = ProvedShared::<Support, Wire> {
        input: literal_input,
        _brand: core::marker::PhantomData,
    };

    let materialize_input = SharedInput::<Wire>::from_exact(&bytes, ()).unwrap();
    let _ = <Opaque as LogicalSchema<'_>>::materialize(materialize_input);
}
