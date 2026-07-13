use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] #[zero(endian="little", endian="big")] struct Bad { #[zero(capacity=4, capacity=5)] x:u32, #[zero(tag_field=missing)] y:u8 }
fn main() {}
