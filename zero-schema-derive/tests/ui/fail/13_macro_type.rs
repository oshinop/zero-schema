use zero_schema_derive::ZeroSchema; macro_rules! ty {()=>{u32}} #[derive(ZeroSchema)] struct Bad { x:ty!() } fn main() {}
