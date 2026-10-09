#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/optional.rs"]
#[allow(dead_code)]
mod optional;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, Priority};
use optional::{OptionalRoot, optional_root_bytes};
use zero_schema::{ErrorKind, LayoutError, SchemaError};

#[repr(align(16))]
struct Slots([u8; producer::ALL_FEATURES_LEN * 2]);

#[test]
fn access_requires_exactly_one_stride_sized_slot() {
    let fixture = producer::all_features_mut();
    let mut slots = Slots([0; producer::ALL_FEATURES_LEN * 2]);
    slots.0[..producer::ALL_FEATURES_LEN].copy_from_slice(fixture.as_bytes());
    slots.0[producer::ALL_FEATURES_LEN..].copy_from_slice(fixture.as_bytes());

    assert_eq!(AllFeatures::SCHEMA_STRIDE, producer::ALL_FEATURES_LEN);
    let first = AllFeatures::access(&slots.0[..AllFeatures::SCHEMA_STRIDE]).unwrap();
    let second =
        AllFeatures::access(&slots.0[AllFeatures::SCHEMA_STRIDE..AllFeatures::SCHEMA_STRIDE * 2])
            .unwrap();
    assert_eq!(
        (first.sequence(), second.sequence()),
        (0x0707_0707_0707_0707, 0x0707_0707_0707_0707)
    );

    let short = AllFeatures::access(&slots.0[..AllFeatures::SCHEMA_SIZE - 1]).unwrap_err();
    assert_eq!(short.kind(), ErrorKind::Layout);
    assert!(
        matches!(std::error::Error::source(&short).and_then(|error| error.downcast_ref::<LayoutError>()), Some(LayoutError::IncorrectSize { expected, actual }) if *expected == AllFeatures::SCHEMA_SIZE && *actual == AllFeatures::SCHEMA_SIZE - 1)
    );
    let extra = AllFeatures::access(&slots.0[..AllFeatures::SCHEMA_SIZE + 1]).unwrap_err();
    assert!(
        matches!(std::error::Error::source(&extra).and_then(|error| error.downcast_ref::<LayoutError>()), Some(LayoutError::IncorrectSize { expected, actual }) if *expected == AllFeatures::SCHEMA_SIZE && *actual == AllFeatures::SCHEMA_SIZE + 1)
    );
    let misaligned = AllFeatures::access(&slots.0[1..AllFeatures::SCHEMA_SIZE + 1]).unwrap_err();
    assert!(matches!(
        std::error::Error::source(&misaligned)
            .and_then(|error| error.downcast_ref::<LayoutError>()),
        Some(LayoutError::Misaligned { required: 16, .. })
    ));
}

#[test]
fn zero_sentinel_root_uses_its_unchanged_exact_wire_extent() {
    #[repr(align(8))]
    struct Slots([u8; OptionalRoot::SCHEMA_SIZE + 1]);

    assert_eq!(
        (OptionalRoot::SCHEMA_SIZE, OptionalRoot::SCHEMA_ALIGN),
        (40, 8)
    );
    let mut slots = Slots([0; OptionalRoot::SCHEMA_SIZE + 1]);
    slots.0[..OptionalRoot::SCHEMA_SIZE].copy_from_slice(&optional_root_bytes());
    assert!(OptionalRoot::access(&slots.0[..OptionalRoot::SCHEMA_SIZE]).is_ok());

    let short = OptionalRoot::access(&slots.0[..OptionalRoot::SCHEMA_SIZE - 1]).unwrap_err();
    assert_eq!(short.kind(), ErrorKind::Layout);
    assert!(matches!(
        std::error::Error::source(&short).and_then(|error| error.downcast_ref::<LayoutError>()),
        Some(LayoutError::IncorrectSize { expected, actual })
            if *expected == OptionalRoot::SCHEMA_SIZE && *actual == OptionalRoot::SCHEMA_SIZE - 1
    ));

    let extra = OptionalRoot::access(&slots.0).unwrap_err();
    assert_eq!(extra.kind(), ErrorKind::Layout);
    assert!(matches!(
        std::error::Error::source(&extra).and_then(|error| error.downcast_ref::<LayoutError>()),
        Some(LayoutError::IncorrectSize { expected, actual })
            if *expected == OptionalRoot::SCHEMA_SIZE && *actual == OptionalRoot::SCHEMA_SIZE + 1
    ));
}

/// A 16-aligned span holding one `AllFeatures` root followed by trailing bytes.
#[repr(C, align(16))]
struct PrefixSpan([u8; producer::ALL_FEATURES_LEN + TRAILING]);

const TRAILING: usize = 32;

fn prefix_span() -> PrefixSpan {
    let fixture = producer::all_features_mut();
    let mut span = PrefixSpan([0xa5; producer::ALL_FEATURES_LEN + TRAILING]);
    span.0[..producer::ALL_FEATURES_LEN].copy_from_slice(fixture.as_bytes());
    span
}

fn layout_error(error: &(impl std::error::Error + 'static)) -> LayoutError {
    *std::error::Error::source(error)
        .and_then(|source| source.downcast_ref::<LayoutError>())
        .expect("a layout failure carries its LayoutError source")
}

#[test]
fn access_prefix_proves_the_leading_schema_span_and_returns_the_remainder() {
    let mut span = prefix_span();
    let size = AllFeatures::SCHEMA_SIZE;

    let (view, remainder) = AllFeatures::access_prefix(&span.0).unwrap();
    assert_eq!(
        view.copy_into(),
        AllFeatures::access(&span.0[..size]).unwrap().copy_into()
    );
    assert_eq!(remainder, &[0xa5; TRAILING]);
    assert_eq!(remainder.as_ptr(), span.0[size..].as_ptr());

    let (_, exact_remainder) = AllFeatures::access_prefix(&span.0[..size]).unwrap();
    assert!(exact_remainder.is_empty());

    // The proof is eager over the prefix and never reads the remainder.
    span.0[producer::all_features_offsets::ACTIVE] = 2;
    let invalid = AllFeatures::access_prefix(&span.0).err().unwrap();
    assert_eq!(invalid.kind(), ErrorKind::InvalidBool);
}

#[test]
fn access_prefix_reports_short_spans_before_alignment() {
    let span = prefix_span();
    let size = AllFeatures::SCHEMA_SIZE;
    let insufficient = |actual| LayoutError::InsufficientBytes {
        required: size,
        actual,
    };

    let short = AllFeatures::access_prefix(&span.0[..size - 1])
        .err()
        .unwrap();
    assert_eq!(short.kind(), ErrorKind::Layout);
    assert_eq!(layout_error(&short), insufficient(size - 1));

    let empty = AllFeatures::access_prefix(&[]).err().unwrap();
    assert_eq!(layout_error(&empty), insufficient(0));

    // A span that is both short and misaligned reports its size first.
    let short_misaligned = AllFeatures::access_prefix(&span.0[1..size]).err().unwrap();
    assert_eq!(layout_error(&short_misaligned), insufficient(size - 1));

    // Alignment still applies to the first byte of a long enough span.
    let misaligned = AllFeatures::access_prefix(&span.0[1..=size]).err().unwrap();
    assert_eq!(misaligned.kind(), ErrorKind::Layout);
    assert!(matches!(
        layout_error(&misaligned),
        LayoutError::Misaligned { required: 16, .. }
    ));
}

#[test]
fn access_mut_prefix_mutates_only_the_prefix_beside_a_disjoint_remainder() {
    let mut span = prefix_span();
    let size = AllFeatures::SCHEMA_SIZE;

    {
        let (mut view, remainder) = AllFeatures::access_mut_prefix(&mut span.0).unwrap();
        view.sequence_mut().set(43).unwrap();
        remainder.fill(0x5a);
        view.name_mut().set("zero").unwrap();
        assert_eq!(remainder.len(), TRAILING);
    }
    let (view, remainder) = AllFeatures::access_prefix(&span.0).unwrap();
    assert_eq!((view.sequence(), view.name()), (43, "zero"));
    assert_eq!(remainder, &[0x5a; TRAILING]);

    let before = span.0;
    let short = AllFeatures::access_mut_prefix(&mut span.0[..size - 1])
        .err()
        .unwrap();
    assert_eq!(short.kind(), ErrorKind::Layout);
    assert_eq!(
        layout_error(&short),
        LayoutError::InsufficientBytes {
            required: size,
            actual: size - 1,
        }
    );
    assert_eq!(span.0, before);

    span.0[producer::all_features_offsets::ACTIVE] = 2;
    let corrupted = span.0;
    let invalid = AllFeatures::access_mut_prefix(&mut span.0).err().unwrap();
    assert_eq!(invalid.kind(), ErrorKind::InvalidBool);
    assert_eq!(span.0, corrupted);
}

#[test]
fn scalar_enum_roots_take_prefixes_with_the_same_contract() {
    let (view, remainder) = Priority::access_prefix(&[3, 0xff, 0xfe]).unwrap();
    assert_eq!((view.get(), remainder), (Priority::High, &[0xff, 0xfe][..]));

    let mut bytes = [3, 0xff, 0xfe];
    {
        let (mut view, remainder) = Priority::access_mut_prefix(&mut bytes).unwrap();
        view.set(Priority::Low).unwrap();
        remainder[0] = 0;
    }
    assert_eq!(bytes, [1, 0, 0xfe]);

    let unknown = Priority::access_prefix(&[9, 1]).err().unwrap();
    assert_eq!(unknown.kind(), ErrorKind::UnknownEnumValue);
    let empty = Priority::access_prefix(&[]).err().unwrap();
    assert_eq!(
        layout_error(&empty),
        LayoutError::InsufficientBytes {
            required: 1,
            actual: 0,
        }
    );
}
