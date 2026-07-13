use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Bad<'a> { a:&'static str, b:&'a mut str, c:&'a [u8], d:(u8,u8), e:[u8;3] }
fn main() {}
