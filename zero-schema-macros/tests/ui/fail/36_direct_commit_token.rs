use zero_schema_macros::zero;
use zs::__private::{ExclusiveInput, SchemaPatch, WireType, WireTypeSupport};

#[zero(crate = zs)]
pub struct PublicRoot {
    value: u8,
}

fn direct_generated_commit(input: ExclusiveInput<'_, <PublicRoot as WireType>::Wire>) {
    type Support = <PublicRoot as WireTypeSupport>::Support;
    <PublicRootPatch as SchemaPatch<Support>>::commit(&PublicRootPatch::default(), input, ());
}

fn main() {}
