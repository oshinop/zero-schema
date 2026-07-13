use widestring::U16Str;
use zero_schema_derive::ZeroSchema;

#[derive(ZeroSchema)]
struct Opposite<'a> {
    #[zero(capacity = 1, endian = "big")]
    value: &'a U16Str,
}

fn main() {}
