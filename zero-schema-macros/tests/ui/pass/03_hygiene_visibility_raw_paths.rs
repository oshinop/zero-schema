#![deny(warnings)]

use zero_schema_macros::zero;

mod outer {
    use super::*;

    #[zero(crate = zs)]
    pub struct RootChild {
        pub value: u8,
    }

    pub mod inner {
        use super::*;

        #[zero(crate = zs)]
        pub struct NestedChild {
            pub value: u8,
        }

        #[zero(crate = zs)]
        pub struct UsesRebasedPaths {
            pub nested: self::NestedChild,
            pub parent: super::RootChild,
        }
    }
}

#[allow(non_camel_case_types)]
#[zero(crate = zs)]
pub struct r#type {
    pub r#match: u8,
}

const _: usize = core::mem::size_of::<
    zerocopy::byteorder::U16<zerocopy::byteorder::LittleEndian>,
>();

fn main() {
    let r#type { r#match } = r#type { r#match: 7 };
    assert_eq!(r#match, 7);
    let _ = outer::inner::UsesRebasedPaths::SCHEMA_SIZE;
    let _ = r#type::SCHEMA_ALIGN;
}
