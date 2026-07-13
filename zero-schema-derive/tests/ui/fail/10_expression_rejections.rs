use zero_schema_derive::ZeroSchema; macro_rules! n {()=>{1}}
#[derive(ZeroSchema)] struct Bad { #[zero(range=(|x|x)(0)..=2)] a:u32, #[zero(must_equal=n!())] b:u32, #[zero(must_equal=Self::X)] c:u32 }
fn main() {}
