use zero_schema_macros::zero;

#[zero]
struct Invalid {
    value: Option<&'static std::ffi::CStr>,
}

fn main() {}
