use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Bad { #[zero(tag_field=payload)] payload:u32, #[zero(tag_field=tag)] tag:u8 }
fn main() {}
