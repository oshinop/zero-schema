use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Generic<'a,const N:usize>{bytes:&'a [u8;N]}
#[derive(ZeroSchema)] struct Parent<'a>{child:Generic<'a,0>,marker:u8}
fn main(){let bytes=[0u8;Parent::WIRE_SIZE];let _=Parent::parse(&bytes);}
