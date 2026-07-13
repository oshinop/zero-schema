use zero_schema::{ErrorKind, SchemaError, ZeroSchema};

#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    value: u32,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Root {
    lead: u8,
    child: Child,
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum Tag {
    Unit = 1,
    Data = 2,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag)]
enum Message {
    #[zero(tag = Tag::Unit)]
    Unit,
    #[zero(tag = Tag::Data)]
    Data(Child),
}
#[derive(ZeroSchema)]
struct External {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Message,
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum UnitTag {
    A = 3,
    B = 4,
}
#[derive(ZeroSchema)]
#[zero(tag = UnitTag)]
enum Units {
    #[zero(tag = UnitTag::A)]
    A,
    #[zero(tag = UnitTag::B)]
    B,
}
#[derive(ZeroSchema)]
struct ExternalUnits {
    marker: u16,
    tag: UnitTag,
    #[zero(tag_field = tag)]
    payload: Units,
}

fn exercise_root()
-> zero_schema::AlignedBytes<<Root as zero_schema::ZeroSchemaType>::Wire, { Root::WIRE_SIZE }> {
    let mut buffer = zero_schema::make_buffer_for!(Root);
    Root {
        lead: 7,
        child: Child { value: 0x1234_5678 },
    }
    .encode_into(buffer.as_bytes_mut())
    .unwrap();
    buffer
}

#[test]
fn prefix_rejects_short_accepts_exact_and_preserves_extra_remainder() {
    let buffer = exercise_root();
    let error = Root::parse_prefix(&buffer.as_bytes()[..Root::WIRE_SIZE - 1]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Layout);
    let (exact, rest) = Root::parse_prefix(buffer.as_bytes()).unwrap();
    assert_eq!(
        exact,
        Root {
            lead: 7,
            child: Child { value: 0x1234_5678 }
        }
    );
    assert!(rest.is_empty());
    let mut extra = buffer.as_bytes().to_vec();
    let suffix = [0xde, 0xad, 0xbe, 0xef];
    extra.extend_from_slice(&suffix);
    let before = extra[Root::WIRE_SIZE..].to_vec();
    let (got, rest) = Root::parse_prefix(&extra).unwrap();
    assert_eq!(got.child.value, 0x1234_5678);
    assert_eq!(rest, suffix);
    assert_eq!(rest, before);
}

#[test]
fn prefix_consumes_wire_size_not_slot_stride() {
    let mut buffer = zero_schema::make_buffer_for!(Message);
    Message::Data(Child { value: 9 })
        .encode_into(buffer.as_bytes_mut())
        .unwrap();
    let suffix = [11, 12, 13, 14, 15];
    let mut input = buffer.as_bytes().to_vec();
    input.extend_from_slice(&suffix);
    let (got, rest) = Message::parse_prefix(&input).unwrap();
    assert_eq!(got, Message::Data(Child { value: 9 }));
    assert_eq!(rest.as_ptr(), input[Message::WIRE_SIZE..].as_ptr());
    assert_eq!(rest, suffix);
    assert_eq!(input.len() - rest.len(), Message::WIRE_SIZE);
}

#[test]
fn nested_and_internal_roots_leave_input_untouched() {
    let root = exercise_root();
    let mut root_input = root.as_bytes().to_vec();
    root_input.extend_from_slice(&[3, 2, 1]);
    let snapshot = root_input.clone();
    let _ = Root::parse_prefix(&root_input).unwrap();
    assert_eq!(root_input, snapshot);
    let mut message = zero_schema::make_buffer_for!(Message);
    Message::Unit.encode_into(message.as_bytes_mut()).unwrap();
    let mut union_input = message.as_bytes().to_vec();
    union_input.extend_from_slice(&[8, 7]);
    let snapshot = union_input.clone();
    let (_, rest) = Message::parse_prefix(&union_input).unwrap();
    assert_eq!(rest, &[8, 7]);
    assert_eq!(union_input, snapshot);
    let external = External {
        tag: Tag::Data,
        payload: Message::Data(Child { value: 17 }),
    };
    let mut b = zero_schema::make_buffer_for!(External);
    external.encode_into(b.as_bytes_mut()).unwrap();
    let mut input = b.as_bytes().to_vec();
    input.extend_from_slice(&[6, 5, 4]);
    let (_, rest) = External::parse_prefix(&input).unwrap();
    assert_eq!(rest, &[6, 5, 4]);
    assert_eq!(input.len() - rest.len(), External::WIRE_SIZE);
    let units = ExternalUnits {
        marker: 0x3344,
        tag: UnitTag::B,
        payload: Units::B,
    };
    let mut b = zero_schema::make_buffer_for!(ExternalUnits);
    units.encode_into(b.as_bytes_mut()).unwrap();
    let mut input = b.as_bytes().to_vec();
    input.extend_from_slice(&[9, 9]);
    let (got, rest) = ExternalUnits::parse_prefix(&input).unwrap();
    assert_eq!(got.marker, 0x3344);
    assert!(matches!(got.payload, Units::B));
    assert_eq!(rest, &[9, 9]);
}
