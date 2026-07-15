use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<&'static widestring::U16Str>,
}

fn main() {}
