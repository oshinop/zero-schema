use zero_schema_macros::zero;

#[zero]
struct Invalid {
    text: Option<&'static str>,
    c_text: Option<&'static std::ffi::CStr>,
    wide: Option<&'static widestring::U16Str>,
    c_wide: Option<&'static widestring::U16CStr>,
    fixed: Option<&'static [u8; 2]>,
}

fn main() {}
