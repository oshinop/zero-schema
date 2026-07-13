use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] #[repr(u8)] enum Tag { A=1 }
#[derive(ZeroSchema)] #[zero(tag=<Tag as Trait>::Assoc)] enum Qualified { #[zero(tag=Tag::A)] A }
#[derive(ZeroSchema)] #[zero(tag=Tag)] enum Repeated { #[zero(tag=Tag::A, tag=Tag::A)] A }
fn main() {}
