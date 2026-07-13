use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)]
struct Generic<'a, const N: usize> { bytes: &'a [u8; N] }
#[derive(ZeroSchema)]
struct Parent<'a> { child: Generic<'a, 1>, marker: u8 }
fn main() {
    let data = [7u8; 1];
    let value = Generic::<1> { bytes: &data };
    let mut root = [0u8; Generic::<1>::WIRE_SIZE];
    value.encode_into(&mut root).unwrap();
    let _ = Generic::<1>::parse(&root).unwrap();
    let parent = Parent { child: value, marker: 9 };
    let mut nested = [0u8; Parent::WIRE_SIZE];
    parent.encode_into(&mut nested).unwrap();
    let _ = Parent::parse(&nested).unwrap();
}
