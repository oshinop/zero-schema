use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Generic<'a,const N:usize>{bytes:&'a [u8;N]}
fn main(){let bytes=[];let _=Generic::<0>::parse(&bytes);}
