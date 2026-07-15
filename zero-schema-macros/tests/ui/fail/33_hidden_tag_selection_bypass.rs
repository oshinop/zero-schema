use zero_schema_macros::zero;
use zs::__private::{
    ExclusiveInput, SharedInput, TaggedMutSelection, TaggedPayloadSupport, TaggedPayloadTypeSupport,
    TaggedRefSelection, WireType, WireTypeSupport,
};

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tag {
    Data = 1,
}

#[zero(crate = zs)]
struct Data {
    value: u32,
}

#[zero(crate = zs)]
enum TaggedData {
    #[zero(tag = Tag::Data)]
    Data(Data),
}

#[zero(crate = zs)]
struct Root {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: TaggedData,
}

#[repr(align(4))]
struct Aligned([u8; 4]);

#[repr(align(4))]
struct RootAligned([u8; 8]);

fn main() {
    type Support = <TaggedData as TaggedPayloadTypeSupport>::Support;
    type Wire = <Support as TaggedPayloadSupport>::Wire;
    type RootSupport = <Root as WireTypeSupport>::Support;
    type RootWire = <Root as WireType>::Wire;

    let storage = Aligned([0; 4]);
    let input = SharedInput::<Wire>::from_exact(&storage.0, ()).unwrap();
    let _ = TaggedRefSelection::<Support>::prove(Tag::Data, input).unwrap();
    let _ = TaggedRefSelection::<Support>::prove_selected(Tag::Data, input, ());
    let root_storage = RootAligned([0; 8]);
    let root = SharedInput::<RootWire>::from_exact(&root_storage.0, ()).unwrap();
    let _ = TaggedRefSelection::<Support>::prove_at::<RootSupport>(root, Tag::Data, 4, ());

    let mut storage = Aligned([0; 4]);
    let input = ExclusiveInput::<Wire>::from_exact(&mut storage.0, ()).unwrap();
    let selection = TaggedMutSelection::<Support>::prove(Tag::Data, input).unwrap();
    let _ = selection.make_mut();
}
