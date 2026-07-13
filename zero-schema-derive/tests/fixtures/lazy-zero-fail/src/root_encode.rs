use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)] struct Generic<'a,const N:usize>{bytes:&'a [u8;N]}
fn main(){let data=[];let value=Generic::<0>{bytes:&data};let mut bytes=[];let _=value.encode_into(&mut bytes);}
