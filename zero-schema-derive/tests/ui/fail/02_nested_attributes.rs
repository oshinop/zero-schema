use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Bad<#[zero(align=2)] T> { value: for<#[zero(align=2)] 'a> fn(&'a T) }
fn main() {}
