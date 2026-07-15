#![deny(warnings)]

use zero_schema_macros::zero;

const ROOT_WIDTH: usize = 2;

mod nested {
    use super::*;

    #[zero(crate = zs)]
    #[derive(Debug)]
    pub struct Child {
        pub value: u8,
    }

    #[zero(crate = zs)]
    pub struct Parent<'a, const N: usize> {
        pub child: self::Child,
        pub bytes: &'a [u8; super::ROOT_WIDTH],
        pub samples: [u16; N],
    }
}

#[zero(crate = zs)]
pub struct GenericParent<T>
where
    T: core::fmt::Debug
        + zs::__private::WireTypeSupport
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + 'static,
{
    pub child: T,
}

fn main() {
    let _ = nested::Parent::<'static, 2>::SCHEMA_SIZE;
    let _ = GenericParent::<nested::Child>::SCHEMA_SIZE;
}
