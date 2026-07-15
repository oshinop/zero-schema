use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZeroValid {
    Zero = 0,
    One = 1,
}

#[zero(crate = zs)]
struct Invalid<T>
where
    T: zs::__private::OptionalWireType
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
{
    value: Option<T>,
}

fn main() {
    let _: core::marker::PhantomData<Invalid<ZeroValid>> = core::marker::PhantomData;
}
