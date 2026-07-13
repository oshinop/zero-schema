use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Generic<'a,const N:usize>{bytes:&'a [u8;N]}
#[derive(ZeroSchema)] struct Parent<'a>{child:Generic<'a,0>,marker:u8}
fn main(){let data=[];let value=Parent{child:Generic{bytes:&data},marker:1};let mut bytes=[0u8;Parent::WIRE_SIZE];let _=value.encode_into(&mut bytes);}
