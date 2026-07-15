use zero_schema_macros::zero;
type U16CStr = zs::__private::U16CStr;

#[zero(crate = zs)]
struct InvalidWideEndian<'a> {
    #[zero(capacity = 2, endian = "little")]
    value: &'a U16CStr,
}

fn main() {}
