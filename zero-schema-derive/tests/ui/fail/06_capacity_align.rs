use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Bad<'a> { #[zero(capacity=256, len_type=u8)] a:&'a str, #[zero(capacity=1u32)] b:&'a str, #[zero(align=3)] c:u32 }
fn main() {}
