use zero_schema_macros::zero;
use zs::__private::{
    ExclusiveInput, OptionFieldAdapter, SchemaPatch, SharedInput, WireType, WireTypeSupport,
};

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Present = 1,
}

#[zero(crate = zs)]
pub struct Child {
    state: State,
}

#[zero(crate = zs)]
pub struct OptionalRoot {
    maybe: Option<Child>,
}

type RootWire = <OptionalRoot as WireType>::Wire;
type RootSupport = <OptionalRoot as WireTypeSupport>::Support;
type Adapter = __zero_schema_support_optionalroot_5161d5ec2fc1a37b::MaybeOptionAdapter;

fn main() {
    let bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let shared_root = SharedInput::<RootWire>::from_exact(&bytes, ()).unwrap();
    let value = shared_root
        .subrange::<<Adapter as OptionFieldAdapter>::ValueWire>(0)
        .unwrap();
    let _ = <Adapter as OptionFieldAdapter>::preflight_init(
        value,
        &Child {
            state: State::Present,
        },
    );

    let mut bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let mut exclusive_root = ExclusiveInput::<RootWire>::from_exact(&mut bytes, ()).unwrap();
    let storage = exclusive_root
        .subrange_mut::<<Adapter as OptionFieldAdapter>::StorageWire>(0)
        .unwrap();
    <Adapter as OptionFieldAdapter>::clear(storage, ());

    let mut bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let mut exclusive_root = ExclusiveInput::<RootWire>::from_exact(&mut bytes, ()).unwrap();
    let value = exclusive_root
        .subrange_mut::<<Adapter as OptionFieldAdapter>::ValueWire>(0)
        .unwrap();
    <Adapter as OptionFieldAdapter>::commit_init(
        value,
        &Child {
            state: State::Present,
        },
        (),
    );

    let shared_root = SharedInput::<RootWire>::from_exact(&bytes, ()).unwrap();
    let _ = <OptionalRootPatch as SchemaPatch<RootSupport>>::preflight_init(
        &OptionalRootPatch::default(),
        shared_root,
    );

    let mut bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let exclusive_root = ExclusiveInput::<RootWire>::from_exact(&mut bytes, ()).unwrap();
    <OptionalRootPatch as SchemaPatch<RootSupport>>::commit_init(
        &OptionalRootPatch::default(),
        exclusive_root,
        (),
    );
}
