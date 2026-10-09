use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    One = 1,
}

// Zero-sentinel eligibility is unchanged: only an array of eligible scalar
// enums or schemas may sit inside `Option`.
#[zero(crate = zs)]
struct OptionalNested {
    values: Option<[[Kind; 2]; 2]>,
}

#[zero(crate = zs)]
struct OptionalStrings<'a> {
    #[zero(capacity = 4)]
    names: Option<[&'a str; 2]>,
}

#[zero(crate = zs)]
struct StringElementOption<'a> {
    #[zero(capacity = 4)]
    names: [Option<&'a str>; 2],
}

fn main() {}
