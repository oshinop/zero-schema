use zero_schema_derive::ZeroSchema;

mod named {
    use super::*;
    #[derive(ZeroSchema)] struct Record { value: u32 }
    #[derive(ZeroSchema)] #[repr(u8)] enum Tag { A = 1 }
    #[derive(ZeroSchema)] #[zero(tag = Tag)] enum Choice { #[zero(tag = Tag::A)] A }
}
mod literals {
    use super::*;
    #[derive(ZeroSchema)] struct Literals<'a> { #[zero(capacity=0x00, len_type=u8, align=0x2)] empty: &'a str, #[zero(capacity=0xff, len_type=u8)] max: &'a str, r#type: u8 }
    #[allow(non_camel_case_types)]
    #[derive(ZeroSchema)] #[repr(u8)] enum Raw { r#type = 1 }
}
mod expressions {
    use super::*;
    const LO:u32=1; const HI:u32=9;
    #[derive(ZeroSchema)] struct R { #[zero(range=(LO + 1)..=(HI as u32), must_equal=(LO + 2))] x:u32 }
}
mod lifetimes {
    use super::*;
    #[derive(ZeroSchema)] #[zero(borrow='wire)] struct Multi<'wire,'other> where 'wire:'other { #[zero(capacity=8)] text:&'other str, marker:&'wire [u8;1] }
}
mod alignment {
    use super::*;
    #[derive(ZeroSchema)] #[repr(C, align(8))] #[zero(align=8)] struct Aligned { x:u8 }
}
fn main() {}
