use zero_schema_macros::zero;

#[zero]
pub struct ZeroValidRecord {
    count: u8,
}

#[zero]
pub struct Invalid<const N: usize> {
    values: Option<[ZeroValidRecord; N]>,
}

fn main() {
    let _: core::marker::PhantomData<Invalid<1>> = core::marker::PhantomData;
}
