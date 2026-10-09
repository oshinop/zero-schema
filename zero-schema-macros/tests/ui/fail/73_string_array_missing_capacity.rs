use zero_schema_macros::zero;

type U16CStr = zs::__private::U16CStr;

#[zero(crate = zs)]
struct MissingCapacity<'a> {
    names: [&'a U16CStr; 4],
}

#[zero(crate = zs)]
struct NestedMissingCapacity<'a> {
    names: [[&'a str; 2]; 2],
}

fn main() {}
