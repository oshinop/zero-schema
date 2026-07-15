use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
enum Kind {
    access = 1,
}

fn main() {}
