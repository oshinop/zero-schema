#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/optional.rs"]
#[allow(dead_code)]
mod optional;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::AllFeatures;
use optional::{OptionalRoot, optional_root_bytes};
use zero_schema::{ErrorKind, LayoutError, SchemaError};

#[repr(align(16))]
struct Slots([u8; producer::ALL_FEATURES_LEN * 2]);

#[test]
fn callers_select_exact_stride_sized_slots_without_prefix_parsing() {
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
