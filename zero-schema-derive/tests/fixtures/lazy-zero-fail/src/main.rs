use zero_schema_derive::ZeroSchema;
#[derive(ZeroSchema)]
struct Generic<'a, const N: usize> { bytes: &'a [u8; N] }
#[derive(ZeroSchema)]
struct Parent<'a> { child: Generic<'a, 0>, marker: u8 }
fn root_parse() { let bytes = []; let _ = Generic::<0>::parse(&bytes); }
fn root_encode() { let data = []; let value = Generic::<0> { bytes: &data }; let mut bytes = []; let _ = value.encode_into(&mut bytes); }
fn parent_parse() { let bytes = [0u8; Parent::WIRE_SIZE]; let _ = Parent::parse(&bytes); }
fn parent_encode() { let data = []; let value = Parent { child: Generic { bytes: &data }, marker: 1 }; let mut bytes = [0u8; Parent::WIRE_SIZE]; let _ = value.encode_into(&mut bytes); }
fn main() { root_parse(); root_encode(); parent_parse(); parent_encode(); }
