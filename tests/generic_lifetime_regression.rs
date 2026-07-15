use zero_schema::{
    __private::{LogicalSchema, OptionalWireType, SchemaPatchType, WireTypeSupport},
    zero,
};
use zero_schema_cross_crate_child::GenericBytes;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GenericLeaf {
    One = 1,
    Two = 2,
}

trait SourceBound<'a> {}

impl<'a> SourceBound<'a> for GenericLeaf {}

#[zero]
struct SourceBoundRecord<'a, T>
where
    T: WireTypeSupport
        + SchemaPatchType
        + for<'view> LogicalSchema<'view>
        + SourceBound<'a>
        + core::fmt::Debug
        + 'static,
{
    #[zero(capacity = 1)]
    name: &'a str,
    child: T,
}

#[zero(borrow = 'a)]
struct SourceBoundOptionalRecord<'a, T, const N: usize>
where
    T: OptionalWireType
        + SchemaPatchType
        + for<'view> LogicalSchema<'view>
        + SourceBound<'a>
        + core::fmt::Debug
        + 'static,
{
    #[zero(capacity = 1)]
    name: &'a str,
    maybe_child: Option<T>,
    maybe_children: Option<[T; N]>,
}

#[zero]
struct GenericParent<'a, const N: usize> {
    child: GenericBytes<'a, N>,
    trailing: u8,
}

#[test]
fn source_bound_record_rebinds_only_its_declared_source_lifetime() {
    #[repr(align(4))]
    struct AlignedBytes([u8; 12]);

    let bytes = AlignedBytes([1, 0, 0, 0, b'x', 0, 0, 0, GenericLeaf::One as u8, 0, 0, 0]);
    assert_eq!(
        (
            SourceBoundRecord::<GenericLeaf>::SCHEMA_SIZE,
            SourceBoundRecord::<GenericLeaf>::SCHEMA_ALIGN
        ),
        (bytes.0.len(), 4),
    );
    let record = SourceBoundRecord::<GenericLeaf>::access(&bytes.0)
        .expect("producer bytes are valid")
        .copy_into();
    assert_eq!(record.name, "x");
    assert_eq!(record.child, GenericLeaf::One);
}

#[test]
fn generic_optional_lifetime_and_const_array_rebind_without_support_projections() {
    #[repr(align(8))]
    struct AlignedBytes([u8; SourceBoundOptionalRecord::<GenericLeaf, 2>::SCHEMA_SIZE]);

    let mut bytes = AlignedBytes([0; SourceBoundOptionalRecord::<GenericLeaf, 2>::SCHEMA_SIZE]);
    {
        let mut record = SourceBoundOptionalRecord::<GenericLeaf, 2>::access_mut(&mut bytes.0)
            .expect("empty source and zero sentinel options are valid");
        record.name_mut().set("x").expect("set source-bound text");
        record
            .maybe_child_mut()
            .set(Some(GenericLeaf::One))
            .expect("initialize generic optional child");
        record
            .maybe_children_mut()
            .set(Some([GenericLeaf::One, GenericLeaf::Two]))
            .expect("initialize generic optional const array");
    }

    let record = SourceBoundOptionalRecord::<GenericLeaf, 2>::access(&bytes.0)
        .expect("initialized generic optionals are valid")
        .copy_into();
    assert_eq!(record.name, "x");
    assert_eq!(record.maybe_child, Some(GenericLeaf::One));
    assert_eq!(
        record.maybe_children,
        Some([GenericLeaf::One, GenericLeaf::Two])
    );
}

#[test]
fn cross_crate_generic_parent_rebinds_its_child_view_lifetime() {
    let bytes = [4, 5, 9];
    let parent = GenericParent::<2>::access(&bytes)
        .expect("producer bytes are valid")
        .copy_into();
    assert_eq!(parent.child.bytes, &[4, 5]);
    assert_eq!(parent.trailing, 9);
}
