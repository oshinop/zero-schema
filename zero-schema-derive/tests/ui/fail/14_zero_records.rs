use zero_schema_derive::ZeroSchema; #[derive(ZeroSchema)] struct Empty {} #[derive(ZeroSchema)] struct StaticZero { x:[u8;0] } fn main() {}
