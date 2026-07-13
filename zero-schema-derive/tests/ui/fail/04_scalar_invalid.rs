use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] #[repr(i16)] enum Repr { A=1 }
#[derive(ZeroSchema)] #[repr(u8)] enum Payload { A(u8) }
#[derive(ZeroSchema)] #[repr(u8)] enum Missing { A }
fn main() {}
