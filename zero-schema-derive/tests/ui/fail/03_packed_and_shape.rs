use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] #[repr(C, packed)] struct Packed { x:u32 }
#[derive(ZeroSchema)] struct Tuple(u8);
fn main() {}
