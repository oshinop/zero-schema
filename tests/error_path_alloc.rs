#![cfg(feature = "alloc")]

use zero_schema::{ErrorPathSegment, SchemaError, ZeroSchema, ZeroSchemaType, error_path_string};

#[derive(Debug, ZeroSchema)]
struct Direct {
    flag: bool,
}
#[derive(Debug, ZeroSchema)]
struct Child {
    bad: bool,
}
#[derive(Debug, ZeroSchema)]
struct Nested {
    child: Child,
}
#[derive(Debug, ZeroSchema)]
#[repr(u8)]
enum Tag {
    Item = 1,
}
#[derive(Debug, ZeroSchema)]
#[zero(tag = Tag)]
enum Selected<T>
where
    T: ZeroSchemaType,
{
    #[zero(tag = Tag::Item)]
    Item(T),
}
#[derive(Debug, ZeroSchema)]
struct GenericParent<T>
where
    T: ZeroSchemaType,
{
    field: Selected<T>,
}
#[derive(Debug, ZeroSchema)]
#[repr(u8)]
enum ExternalTag {
    Item = 1,
    Spare = 2,
}
#[derive(Debug, ZeroSchema)]
#[zero(tag = ExternalTag)]
enum ExternalMessage {
    #[zero(tag = ExternalTag::Item)]
    Item(Child),
}
#[derive(Debug, ZeroSchema)]
struct Envelope {
    tag: ExternalTag,
    #[zero(tag_field = tag)]
    payload: ExternalMessage,
}

fn assert_path(error: &dyn SchemaError, expected: &str) {
    let owned = error_path_string(error);
    assert_eq!(owned, expected);
    let display = error.to_string();
    assert_eq!(
        display
            .strip_suffix(display.split_once(": ").unwrap().1)
            .unwrap()
            .strip_suffix(": ")
            .unwrap(),
        expected
    );
    assert_eq!(display.matches(error.schema()).count(), 1);
    let names = expected.split('.').count();
    let mut current = error;
    let mut traversed = 1;
    while let Some(child) = current.child() {
        if current.segment().is_some() {
            traversed += 1;
        }
        current = child;
    }
    if current.segment().is_some() {
        traversed += 1;
    }
    assert_eq!(traversed, names);
}

#[test]
fn allocated_paths_match_direct_nested_and_generic_traversal() {
    let mut direct = zero_schema::make_buffer_for!(Direct);
    direct.as_bytes_mut()[0] = 2;
    let error = Direct::parse(direct.as_bytes()).unwrap_err();
    assert_path(&error, "Direct.flag");

    let mut nested = zero_schema::make_buffer_for!(Nested);
    nested.as_bytes_mut()[0] = 2;
    let error = Nested::parse(nested.as_bytes()).unwrap_err();
    assert_path(&error, "Nested.child.bad");
    let child = error.child().unwrap();
    assert_eq!(
        (child.schema(), child.segment()),
        ("Child", Some(ErrorPathSegment::Field("bad")))
    );

    let value = GenericParent {
        field: Selected::Item(Child { bad: true }),
    };
    let mut storage = zero_schema::make_buffer_for!(GenericParent<Child>);
    value.encode_into(storage.as_bytes_mut()).unwrap();
    let selected_offset = GenericParent::<Child>::LAYOUT.fields()[0].offset();
    let payload_offset = match Selected::<Child>::LAYOUT.kind() {
        zero_schema::TypeKind::TaggedUnion { payload_offset, .. } => payload_offset,
        _ => unreachable!(),
    };
    storage.as_bytes_mut()[selected_offset + payload_offset] = 2;
    let error = GenericParent::<Child>::parse(storage.as_bytes()).unwrap_err();
    assert_path(&error, "GenericParent.field.Item.bad");
    let selected = error.child().unwrap();
    assert_eq!(
        (selected.schema(), selected.segment()),
        ("Selected", Some(ErrorPathSegment::Variant("Item")))
    );
    let leaf = selected.child().unwrap();
    assert_eq!(
        (leaf.schema(), leaf.segment()),
        ("Child", Some(ErrorPathSegment::Field("bad")))
    );
}

#[test]
fn allocated_external_path_has_no_duplicate_child_schema() {
    let mut storage = zero_schema::make_buffer_for!(Envelope);
    let tag_offset = Envelope::LAYOUT
        .fields()
        .iter()
        .find(|f| f.name() == "tag")
        .unwrap()
        .offset();
    storage.as_bytes_mut()[tag_offset] = 2;
    let error = Envelope::parse(storage.as_bytes()).unwrap_err();
    assert_path(&error, "Envelope.payload");
    let child = error.child().unwrap();
    assert_eq!((child.schema(), child.segment()), ("ExternalMessage", None));
}
