use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] #[repr(u8)] enum Scalar { Wire=1 }
#[derive(ZeroSchema)] #[repr(u8)] enum Tag { Parse=1 }
#[derive(ZeroSchema)] #[zero(tag=Tag)] enum Union { #[zero(tag=Tag::Parse)] Wire }
fn main() {}
