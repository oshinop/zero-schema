use zero_schema as zs;
#[cfg(not(miri))]
use zero_schema_cross_crate_child::BorrowedChild;
use zero_schema_cross_crate_child::{
    BigCode, ChildMessage, ChildTag, DirectChild, GenericBytes, LittleCode, NativeCode,
    OptionalChild, OptionalChildPatch, OptionalCode, TrailingProjection,
};
use zs::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError, zero};

#[derive(Debug, Eq, PartialEq)]
pub struct ErrorFacts {
    pub display: String,
    pub kind: ErrorKind,
    pub schema: &'static str,
    pub segment: Option<ErrorPathSegment>,
    pub source: Option<LayoutError>,
    pub child_schema: Option<&'static str>,
    pub child_source_identical: bool,
}

fn facts<E: SchemaError>(error: E) -> ErrorFacts {
    let child = error.child();
    let source_error = error.source();
    ErrorFacts {
        display: error.to_string(),
        kind: error.kind(),
        schema: error.schema(),
        segment: error.segment(),
        source: source_error
            .and_then(|source| source.downcast_ref::<LayoutError>())
            .copied(),
        child_schema: child.map(SchemaError::schema),
        child_source_identical: match (child, source_error) {
            (Some(child), Some(source)) => core::ptr::eq(
                child as *const dyn SchemaError as *const (),
                source as *const dyn std::error::Error as *const (),
            ),
            _ => false,
        },
    }
}

pub fn read_big(bytes: &[u8]) -> Result<u16, ErrorFacts> {
    BigCode::access(bytes)
        .map(|value| match value.get() {
            BigCode::Ready => 0x0102,
            BigCode::r#type => 0xabcd,
        })
        .map_err(facts)
}

pub fn read_little(bytes: &[u8]) -> Result<u32, ErrorFacts> {
    LittleCode::access(bytes)
        .map(|value| match value.get() {
            LittleCode::First => 0x0102_0304,
            LittleCode::Last => 0xffff_fffe,
        })
        .map_err(facts)
}

pub fn read_native(bytes: &[u8]) -> Result<u32, ErrorFacts> {
    NativeCode::access(bytes)
        .map(|value| match value.get() {
            NativeCode::Marker => 0x1122_3344,
            NativeCode::Maximum => 0xffff_ffff,
        })
        .map_err(facts)
}

pub fn big_unknown_facts(bytes: &[u8]) -> ErrorFacts {
    BigCode::access(bytes)
        .err()
        .map(facts)
        .expect("unknown scalar value")
}

pub fn big_layout_facts(bytes: &[u8]) -> ErrorFacts {
    BigCode::access(bytes)
        .err()
        .map(facts)
        .expect("scalar layout error")
}

mod private_parent {
    use super::*;

    #[zero(crate = crate::zs)]
    pub struct PrivateChild {
        valid: bool,
        code: BigCode,
    }

    #[zero(crate = crate::zs)]
    pub struct PublicPrivateParent {
        prefix: u8,
        child: PrivateChild,
    }

    pub(super) fn observation(bytes: &[u8]) -> super::Observation {
        let mut storage = zs::schema_buffer!(PublicPrivateParent);
        storage.as_bytes_mut().copy_from_slice(bytes);
        let copied = PublicPrivateParent::access(storage.as_bytes())
            .expect("reviewed producer fixture")
            .copy_into();
        super::Observation {
            prefix: copied.prefix,
            valid: copied.child.valid,
        }
    }

    pub(super) fn error_facts(bytes: &[u8]) -> super::ErrorFacts {
        PublicPrivateParent::access(bytes)
            .err()
            .map(super::facts)
            .expect("invalid private child")
    }
}

#[zero(crate = crate::zs)]
pub struct PublicParent {
    prefix: u8,
    child: DirectChild,
}

#[cfg(not(miri))]
#[zero(crate = crate::zs, borrow = 'a)]
pub struct BorrowingParent<'a> {
    child: BorrowedChild<'a>,
    marker: &'a [u8; 2],
}

#[zero(crate = crate::zs)]
pub struct GenericParent<'a, const N: usize> {
    child: GenericBytes<'a, N>,
    trailing: u8,
}

#[zero(crate = crate::zs)]
pub struct CompositionParent {
    direct: DirectChild,
    projected: TrailingProjection,
    child_tag: ChildTag,
    #[zero(tag_field = child_tag)]
    tagged: ChildMessage,
}

/// Downstream composition uses only the public logical child declarations;
/// their generated support remains private to the child crate.
#[zero(crate = crate::zs)]
pub struct OptionalParent {
    maybe_code: Option<OptionalCode>,
    maybe_child: Option<OptionalChild>,
    maybe_codes: Option<[OptionalCode; 2]>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    pub prefix: u8,
    pub valid: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct OptionalObservation {
    pub code: Option<OptionalCode>,
    pub child: Option<(OptionalCode, u16)>,
    pub codes: Option<[OptionalCode; 2]>,
}

pub fn public_parent_fixture() -> &'static [u8] {
    include_bytes!("../golden/public-parent.bin")
}

pub fn private_parent_fixture() -> &'static [u8] {
    include_bytes!("../golden/private-parent.bin")
}

pub fn composition_parent_fixture() -> &'static [u8] {
    include_bytes!("../golden/composition-parent.bin")
}

pub fn public_parent_from_fixture() -> Observation {
    let mut storage = zs::schema_buffer!(PublicParent);
    storage
        .as_bytes_mut()
        .copy_from_slice(public_parent_fixture());
    let view = PublicParent::access(storage.as_bytes()).expect("reviewed producer fixture");
    let copied = view.copy_into();
    Observation {
        prefix: copied.prefix,
        valid: copied.child.valid,
    }
}

pub fn private_parent_from_fixture() -> Observation {
    private_parent::observation(private_parent_fixture())
}

pub fn truly_private_error_facts(bytes: &[u8]) -> ErrorFacts {
    private_parent::error_facts(bytes)
}

pub fn private_error_facts(bytes: &[u8]) -> ErrorFacts {
    PublicParent::access(bytes)
        .err()
        .map(facts)
        .expect("invalid private child")
}

pub fn child_message_unknown_facts(bytes: &[u8]) -> ErrorFacts {
    CompositionParent::access(bytes)
        .err()
        .map(facts)
        .expect("unknown external child tag")
}

pub fn composition_metadata() -> (usize, usize, usize, usize, &'static str) {
    let layout = CompositionParent::LAYOUT;
    let payload = &layout.fields()[3];
    (
        layout.size(),
        layout.align(),
        layout.fields()[2].offset(),
        payload.offset(),
        payload.name(),
    )
}

pub fn generic_layout<const N: usize>() -> (usize, usize, usize) {
    (
        GenericParent::<'static, N>::SCHEMA_SIZE,
        GenericParent::<'static, N>::LAYOUT.fields()[0].size(),
        GenericBytes::<'static, N>::LAYOUT.fields()[0].size(),
    )
}

fn optional_observation(logical: OptionalParent) -> OptionalObservation {
    OptionalObservation {
        code: logical.maybe_code,
        child: logical.maybe_child.map(|child| (child.code, child.payload)),
        codes: logical.maybe_codes,
    }
}

/// Proves an all-zero downstream option span materializes without exposing
/// child support or wire projections in this public API.
pub fn optional_parent_none_from_zeroed() -> OptionalObservation {
    let storage = zs::schema_buffer!(OptionalParent);
    optional_observation(
        OptionalParent::access(storage.as_bytes())
            .expect("zero sentinel option fields are absent")
            .copy_into(),
    )
}

/// Exercises downstream optional mutation, nested logical patches, and a
/// patch clear through public logical child declarations only.
pub fn optional_parent_mutation_and_patch() -> OptionalObservation {
    let mut storage = zs::schema_buffer!(OptionalParent);
    {
        let mut parent = OptionalParent::access_mut(storage.as_bytes_mut())
            .expect("zero sentinel option fields are absent");
        parent
            .maybe_code_mut()
            .set(Some(OptionalCode::One))
            .expect("initialize optional enum");
        parent
            .maybe_child_mut()
            .set(Some(OptionalChild {
                code: OptionalCode::Two,
                payload: 0x1234,
            }))
            .expect("initialize optional child");
        parent
            .maybe_codes_mut()
            .set(Some([OptionalCode::One, OptionalCode::Two]))
            .expect("initialize optional enum array");

        let patch = OptionalParentPatch {
            maybe_code: Some(Some(OptionalCode::Two.into())),
            maybe_child: Some(Some(OptionalChildPatch {
                code: Some(OptionalCode::One.into()),
                payload: Some(0x5678),
            })),
            maybe_codes: Some(Some([OptionalCode::Two, OptionalCode::One])),
        };
        parent
            .copy_from(&patch)
            .expect("complete patch updates and clears present optionals");
    }
    optional_observation(
        OptionalParent::access(storage.as_bytes())
            .expect("patched optional fields stay valid")
            .copy_into(),
    )
}

pub fn composition_from_fixture() -> (u8, u16, u8, u32) {
    let mut storage = zs::schema_buffer!(CompositionParent);
    storage
        .as_bytes_mut()
        .copy_from_slice(composition_parent_fixture());
    let view = CompositionParent::access(storage.as_bytes()).expect("reviewed producer fixture");
    let _ = view.copy_into();
    let tagged = view.tagged();
    let data = tagged.data().expect("Data is selected by the external tag");
    (
        u8::from(view.direct().valid()),
        view.projected().child().value(),
        view.projected().sentinel(),
        data.number(),
    )
}

#[cfg(test)]
mod opaque_private_parent_regression {
    use super::*;

    #[repr(align(16))]
    struct Aligned<const N: usize>([u8; N]);

    #[test]
    fn private_parent_materializes_without_naming_the_child_projection() {
        assert_eq!(
            private_parent_from_fixture(),
            Observation {
                prefix: 11,
                valid: false,
            }
        );
    }

    #[test]
    fn private_parent_error_preserves_the_private_child_boundary() {
        let mut malformed = Aligned(*include_bytes!("../golden/private-parent.bin"));
        malformed.0[2] = 2;
        let error = truly_private_error_facts(&malformed.0);
        assert_eq!(error.kind, ErrorKind::InvalidBool);
        assert_eq!(error.segment, Some(ErrorPathSegment::Field("child")));
        assert_eq!(error.child_schema, Some("PrivateChild"));
        assert!(error.child_source_identical);
    }
}
