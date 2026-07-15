use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    First = 1,
    Second = 2,
}

#[zero(crate = zs)]
enum VariantMethodCollision {
    #[zero(tag = Kind::First)]
    Data,
    #[allow(non_camel_case_types)]
    #[zero(tag = Kind::Second)]
    data,
}

fn main() {}
