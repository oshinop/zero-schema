use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZeroValidScalar {
    Zero = 0,
    One = 1,
}

#[zero]
pub struct Invalid<const N: usize> {
    values: Option<[ZeroValidScalar; N]>,
}

fn main() {
    let _: core::marker::PhantomData<Invalid<1>> = core::marker::PhantomData;
}
