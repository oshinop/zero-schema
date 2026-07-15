use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<&'static widestring::U16CStr>,
}

fn main() {}
