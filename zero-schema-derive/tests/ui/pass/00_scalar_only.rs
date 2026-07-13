use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)]
#[repr(u8)]
enum Scalar { Value = 1 }
fn main() { let _ = Scalar::Value; }
