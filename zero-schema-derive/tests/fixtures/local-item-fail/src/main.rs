use zero_schema_derive::ZeroSchema;
fn main() {
    #[derive(ZeroSchema)]
    struct Local { x: u8 }
    let value = Local { x: 1 };
    let mut bytes = [0u8; Local::WIRE_SIZE];
    value.encode_into(&mut bytes).unwrap();
    let parsed = Local::parse(&bytes).unwrap();
    assert_eq!(parsed.x, 1);

    #[derive(ZeroSchema)]
    #[repr(u8)]
    enum Tag { A = 1 }
    #[derive(ZeroSchema)]
    #[zero(tag = Tag)]
    enum Choice { #[zero(tag = Tag::A)] A(Local) }
    let choice = Choice::A(Local { x: 2 });
    let mut tagged = [0u8; Choice::WIRE_SIZE];
    choice.encode_into(&mut tagged).unwrap();
    match Choice::parse(&tagged).unwrap() {
        Choice::A(local) => assert_eq!(local.x, 2),
    }
}
