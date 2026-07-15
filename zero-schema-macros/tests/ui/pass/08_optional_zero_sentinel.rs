#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
    Two = 2,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Child {
    required: Required,
}

#[zero(crate = zs)]
pub struct OptionalRoot {
    #[zero(align = 8)]
    maybe_kind: Option<Required>,
    maybe_child: Option<Child>,
    maybe_array: Option<[Required; 2]>,
    maybe_core: core::option::Option<Required>,
    maybe_root_core: ::core::option::Option<Required>,
    maybe_std: std::option::Option<Required>,
    maybe_root_std: ::std::option::Option<Required>,
}

#[zero(crate = zs)]
pub struct GenericOptional<
    T: zs::__private::OptionalWireType
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
> {
    maybe: Option<T>,
}

pub fn exercise() {
    let mut bytes = [0_u8; OptionalRoot::SCHEMA_SIZE];
    let read = OptionalRoot::access(&bytes).expect("zero sentinel is absent");
    assert_eq!(read.maybe_kind(), None);
    assert!(read.maybe_child().is_none());
    assert!(read.maybe_array().is_none());
    assert!(read.maybe_core().is_none());
    assert!(read.maybe_root_core().is_none());
    assert!(read.maybe_std().is_none());
    assert!(read.maybe_root_std().is_none());

    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("zero sentinel is absent");
        root.maybe_kind_mut()
            .set(Some(Required::One))
            .expect("initialize enum");
        assert_eq!(root.maybe_kind(), Some(Required::One));
        root.maybe_kind_mut().set(None).expect("clear enum");
        root.maybe_child_mut()
            .set(Some(Child {
                required: Required::Two,
            }))
            .expect("initialize child");
        root.maybe_array_mut()
            .set(Some([Required::One, Required::Two]))
            .expect("initialize array");
        let patch = OptionalRootPatch {
            maybe_kind: None,
            maybe_child: Some(Some(ChildPatch {
                required: Some(Required::One.into()),
            })),
            maybe_array: Some(None),
            maybe_core: None,
            maybe_root_core: None,
            maybe_std: None,
            maybe_root_std: None,
        };
        root.copy_from(&patch).expect("present patch");
    }

    let copied = OptionalRoot::access(&bytes)
        .expect("optional storage stays valid")
        .copy_into();
    assert_eq!(copied.maybe_kind, None);
    assert_eq!(
        copied.maybe_child,
        Some(Child {
            required: Required::One
        })
    );
    assert_eq!(copied.maybe_array, None);
    assert_eq!(copied.maybe_core, None);
    assert_eq!(copied.maybe_root_core, None);
    assert_eq!(copied.maybe_std, None);
    assert_eq!(copied.maybe_root_std, None);
    let _: core::marker::PhantomData<GenericOptional<Required>> = core::marker::PhantomData;
}

#[allow(dead_code)]
fn main() {
    exercise();
}
