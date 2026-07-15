use zero_schema_macros::zero;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    Value = 1,
}

#[zero(crate = zs)]
enum Payload {
    #[zero(tag = Kind::Value)]
    Value,
}

fn main() {
    let _ = Payload::access(&[0_u8]);
    let _ = Payload::access_mut(&mut [0_u8]);
    let _ = Payload::SCHEMA_SIZE;
}
