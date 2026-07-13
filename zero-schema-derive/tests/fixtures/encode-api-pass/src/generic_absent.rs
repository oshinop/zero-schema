use zero_schema_derive::ZeroSchema;

#[derive(ZeroSchema)]
#[zero(crate = zs)]
struct Generic<T: zs::ZeroSchemaType> {
    value: T,
}

#[derive(ZeroSchema)]
#[zero(crate = zs)]
struct ConstGeneric<'a, const N: usize> {
    bytes: &'a [u8; N],
}

fn main() {
    let _ = Generic { value: 1u16 }.encode();
    let _ = ConstGeneric::<2> { bytes: &[1, 2] }.encode();
}
