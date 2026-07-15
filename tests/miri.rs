#[path = "support/optional.rs"]
#[allow(dead_code)]
mod optional;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use core::{error::Error as _, ffi::CStr};

use widestring::{U16CStr, U16Str};
use zero_schema::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError, zero};

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConfigKind {
    File = 1,
    Memory = 2,
    Reserved = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    version: u16,
    producer: [u8; 6],
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryConfig {
    capacity: u16,
    enabled: bool,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileConfig<'a> {
    version: u16,
    #[zero(capacity = 6)]
    producer: &'a CStr,
    flags: u32,
}

#[zero]
#[derive(Debug, PartialEq)]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
}

#[zero(align = 16)]
#[derive(Debug, PartialEq)]
pub struct AllFeatures<'a> {
    sequence: u64,
    active: bool,
    priority: Priority,
    #[zero(capacity = 7, len_type = u8)]
    name: &'a str,
    #[zero(capacity = 6)]
    c_name: &'a CStr,
    #[zero(capacity = 2, len_type = u8, align = 4)]
    wide: &'a U16Str,
    #[zero(capacity = 3)]
    wide_c: &'a U16CStr,
    token: &'a [u8; 5],
    header: Header,
    samples: [u32; 3],
    headers: [Header; 2],
    config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    config: Config<'a>,
    checksum: u8,
}

fn fixture() -> producer::AlignedAllFeatures {
    let bytes = producer::all_features_mut();
    assert!(bytes.is_exactly_aligned());
    bytes
}

fn snapshot(bytes: &producer::AlignedAllFeatures) -> [u8; producer::ALL_FEATURES_LEN] {
    bytes
        .as_bytes()
        .try_into()
        .expect("fixture length is exact")
}

fn assert_unchanged(
    bytes: &producer::AlignedAllFeatures,
    before: &[u8; producer::ALL_FEATURES_LEN],
) {
    assert_eq!(
        bytes.as_bytes(),
        before,
        "failed operation changed any wire byte"
    );
}

fn assert_kind_and_path<E: SchemaError>(error: &E, kind: ErrorKind, path: &[ErrorPathSegment]) {
    assert_eq!(error.kind(), kind, "unexpected error: {error}");
    let mut current: &dyn SchemaError = error;
    for (index, segment) in path.iter().enumerate() {
        assert_eq!(
            current.segment(),
            Some(*segment),
            "unexpected path: {error}"
        );
        if index + 1 != path.len() {
            current = current.child().expect("missing nested error path segment");
        }
    }
}

#[test]
fn capabilities_borrow_producer_storage_through_root_nested_array_and_union() {
    let bytes = fixture();
    let view = AllFeatures::access(bytes.as_bytes()).expect("reviewed producer bytes are valid");

    assert_eq!(view.sequence(), 0x0707_0707_0707_0707);
    assert!(view.active());
    assert_eq!(view.priority(), Priority::High);
    assert_eq!(view.name(), "api");
    assert_eq!(view.c_name().to_bytes(), b"svc");
    assert_eq!(
        view.samples().copy_into(),
        [0x1111_1111, 0x1212_1212, 0x1313_1313]
    );
    assert_eq!(
        view.headers().get(1).expect("second header").version(),
        0x2525
    );

    assert_eq!(
        view.name().as_ptr(),
        bytes.as_bytes()[producer::all_features_offsets::NAME + 1..].as_ptr(),
        "root string must borrow producer storage"
    );
    assert_eq!(
        view.token().as_ptr(),
        bytes.as_bytes()[producer::all_features_offsets::TOKEN..].as_ptr(),
        "root fixed-byte array must borrow producer storage"
    );

    let copied = view.copy_into();
    assert_eq!(copied.sequence, view.sequence());
    assert_eq!(copied.samples, view.samples().copy_into());
    assert_eq!(copied.header.version, view.header().version());
    assert!(matches!(
        copied.config,
        Config::Memory(MemoryConfig {
            capacity: 0x3333,
            enabled: true,
        })
    ));
    let config = view.config();
    assert_eq!(config.tag(), ConfigKind::Memory);
    assert!(config.file().is_none());
    assert_eq!(
        config.memory().expect("selected payload").capacity(),
        0x3333
    );
}

#[test]
fn access_rejects_only_exact_aligned_spans_without_changing_them() {
    #[repr(align(16))]
    struct Storage([u8; producer::ALL_FEATURES_LEN + 1]);

    let valid = fixture();
    let mut storage = Storage([0; producer::ALL_FEATURES_LEN + 1]);
    storage.0[..producer::ALL_FEATURES_LEN].copy_from_slice(valid.as_bytes());
    let before = storage.0;

    let short = AllFeatures::access(&storage.0[..producer::ALL_FEATURES_LEN - 1]).unwrap_err();
    assert_kind_and_path(&short, ErrorKind::Layout, &[]);
    match short
        .source()
        .and_then(|source| source.downcast_ref::<LayoutError>())
    {
        Some(LayoutError::IncorrectSize { expected, actual }) => {
            assert_eq!(
                (*expected, *actual),
                (producer::ALL_FEATURES_LEN, producer::ALL_FEATURES_LEN - 1)
            );
        }
        other => panic!("unexpected short-input layout error: {other:?}"),
    }

    let extra = AllFeatures::access(&storage.0).unwrap_err();
    assert_kind_and_path(&extra, ErrorKind::Layout, &[]);
    match extra
        .source()
        .and_then(|source| source.downcast_ref::<LayoutError>())
    {
        Some(LayoutError::IncorrectSize { expected, actual }) => {
            assert_eq!(
                (*expected, *actual),
                (producer::ALL_FEATURES_LEN, producer::ALL_FEATURES_LEN + 1)
            );
        }
        other => panic!("unexpected extra-input layout error: {other:?}"),
    }

    let misaligned = AllFeatures::access(&storage.0[1..]).unwrap_err();
    assert_kind_and_path(&misaligned, ErrorKind::Layout, &[]);
    assert!(matches!(
        misaligned
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::Misaligned { required: 16, .. })
    ));
    assert_eq!(
        storage.0, before,
        "failed access must preserve every source byte"
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::ACTIVE] = 2;
    let before = snapshot(&invalid);
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::InvalidBool,
        &[ErrorPathSegment::Field("active")],
    );
    assert_unchanged(&invalid, &before);
}

#[test]
fn mutable_capability_reborrows_and_selected_payload_mutation_remain_valid() {
    let mut bytes = fixture();
    {
        let mut view = AllFeatures::access_mut(bytes.as_bytes_mut())
            .expect("reviewed producer bytes are valid");
        let name_pointer = view.name().as_ptr();

        view.sequence_mut().set(43).expect("scalar mutation");
        view.active_mut().set(false).expect("boolean mutation");
        view.priority_mut()
            .set(Priority::Normal)
            .expect("enum mutation");
        {
            let mut nested = view.header_mut();
            nested.version_mut().set(0x4444).expect("nested mutation");
        }
        {
            let mut samples = view.samples_mut();
            samples
                .get_mut(1)
                .expect("valid element")
                .set(21)
                .expect("array element mutation");
            samples.copy_from(&[19, 21, 23]).expect("array copy");
        }
        {
            let mut headers = view.headers_mut();
            headers
                .get_mut(1)
                .expect("valid nested array element")
                .version_mut()
                .set(0x5555)
                .expect("nested array element mutation");
        }
        {
            let mut config = view.config_mut();
            let mut memory = config.memory_mut().expect("Memory is selected");
            memory
                .capacity_mut()
                .set(0x7777)
                .expect("selected payload scalar mutation");
            memory
                .enabled_mut()
                .set(false)
                .expect("selected payload boolean mutation");
        }

        assert_eq!(
            view.name().as_ptr(),
            name_pointer,
            "shared read reborrow remains tied to the same storage"
        );
        assert_eq!(view.header().version(), 0x4444);
        assert_eq!(view.samples().copy_into(), [19, 21, 23]);
        assert_eq!(
            view.headers()
                .get(1)
                .expect("same nested array element")
                .version(),
            0x5555
        );
        assert_eq!(
            view.config()
                .memory()
                .expect("same selected payload")
                .capacity(),
            0x7777
        );
    }

    let view =
        AllFeatures::access(bytes.as_bytes()).expect("successful mutation leaves a valid wire");
    assert_eq!(
        (view.sequence(), view.active(), view.priority()),
        (43, false, Priority::Normal)
    );
    assert_eq!(view.header().version(), 0x4444);
    assert_eq!(view.samples().copy_into(), [19, 21, 23]);
    assert_eq!(
        view.headers()
            .get(1)
            .expect("fresh nested array element")
            .version(),
        0x5555
    );
    let selected = view.config().memory().expect("Memory remains selected");
    assert_eq!((selected.capacity(), selected.enabled()), (0x7777, false));
}

#[test]
fn patches_switch_selected_union_after_payload_and_preserve_all_bytes_on_errors() {
    let mut bytes = fixture();
    {
        let mut view = AllFeatures::access_mut(bytes.as_bytes_mut())
            .expect("reviewed producer bytes are valid");
        view.copy_from(&AllFeaturesPatch::default())
            .expect("no-op patch");
        view.copy_from(&AllFeaturesPatch {
            config: Some(ConfigPatch::Memory(MemoryConfigPatch {
                capacity: Some(0x9999),
                enabled: None,
            })),
            ..Default::default()
        })
        .expect("same-variant partial patch");
        view.copy_from(&AllFeaturesPatch {
            config: Some(ConfigPatch::File(FileConfigPatch {
                version: Some(0x8888),
                producer: Some(c"file"),
                flags: Some(0x0102_0304),
            })),
            ..Default::default()
        })
        .expect("complete switch derives its external tag");
    }
    let switched =
        AllFeatures::access(bytes.as_bytes()).expect("successful switch leaves valid bytes");
    assert_eq!(switched.config_kind(), ConfigKind::File);
    let file = switched.config().file().expect("File payload is selected");
    assert_eq!(
        (file.version(), file.producer().to_bytes(), file.flags()),
        (0x8888, b"file".as_slice(), 0x0102_0304)
    );
    assert_eq!(
        file.producer().as_ptr().cast::<u8>(),
        bytes.as_bytes()[producer::all_features_offsets::CONFIG + 2..].as_ptr(),
        "selected payload must borrow the active union bytes"
    );

    let mut tag_only = fixture();
    let before = snapshot(&tag_only);
    {
        let mut view = AllFeatures::access_mut(tag_only.as_bytes_mut()).expect("valid fixture");
        let error = view
            .copy_from(&AllFeaturesPatch {
                config_kind: Some(ConfigKind::File),
                config: None,
                ..Default::default()
            })
            .unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::TagOnlyPatch,
            &[ErrorPathSegment::Field("config")],
        );
    }
    assert_unchanged(&tag_only, &before);

    let mut incomplete = fixture();
    let before = snapshot(&incomplete);
    {
        let mut view = AllFeatures::access_mut(incomplete.as_bytes_mut()).expect("valid fixture");
        let error = view
            .copy_from(&AllFeaturesPatch {
                config_kind: Some(ConfigKind::File),
                config: Some(ConfigPatch::File(FileConfigPatch {
                    version: Some(7),
                    producer: None,
                    flags: Some(9),
                })),
                ..Default::default()
            })
            .unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::IncompleteUnionSwitch,
            &[ErrorPathSegment::Field("config")],
        );
    }
    assert_unchanged(&incomplete, &before);

    let mut mismatch = fixture();
    let before = snapshot(&mismatch);
    {
        let mut view = AllFeatures::access_mut(mismatch.as_bytes_mut()).expect("valid fixture");
        let error = view
            .copy_from(&AllFeaturesPatch {
                config_kind: Some(ConfigKind::File),
                config: Some(ConfigPatch::Memory(MemoryConfigPatch {
                    capacity: Some(9),
                    enabled: Some(false),
                })),
                ..Default::default()
            })
            .unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::TagMismatch,
            &[ErrorPathSegment::Field("config")],
        );
    }
    assert_unchanged(&mismatch, &before);
}

#[test]
fn every_failed_mutation_preflight_preserves_the_entire_producer_fixture() {
    let mut string = fixture();
    let before = snapshot(&string);
    {
        let mut view = AllFeatures::access_mut(string.as_bytes_mut()).expect("valid fixture");
        let error = view.name_mut().set("overlong").unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::CapacityExceeded,
            &[ErrorPathSegment::Field("name")],
        );
    }
    assert_unchanged(&string, &before);

    let mut fixed_bytes = fixture();
    let before = snapshot(&fixed_bytes);
    {
        let mut view = AllFeatures::access_mut(fixed_bytes.as_bytes_mut()).expect("valid fixture");
        let error = view.token_mut().set(b"tiny").unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::ArrayLengthMismatch,
            &[ErrorPathSegment::Field("token")],
        );
    }
    assert_unchanged(&fixed_bytes, &before);

    let mut index = fixture();
    let before = snapshot(&index);
    {
        let mut view = AllFeatures::access_mut(index.as_bytes_mut()).expect("valid fixture");
        let error = view.samples_mut().set(3, 99).unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::ArrayIndexOutOfBounds,
            &[
                ErrorPathSegment::Field("samples"),
                ErrorPathSegment::Index(3),
            ],
        );
    }
    assert_unchanged(&index, &before);

    let mut length = fixture();
    let before = snapshot(&length);
    {
        let mut view = AllFeatures::access_mut(length.as_bytes_mut()).expect("valid fixture");
        let error = view.samples_mut().copy_from(&[1, 2]).unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::ArrayLengthMismatch,
            &[ErrorPathSegment::Field("samples")],
        );
    }
    assert_unchanged(&length, &before);

    let mut selected = fixture();
    let before = snapshot(&selected);
    {
        let mut view = AllFeatures::access_mut(selected.as_bytes_mut()).expect("valid fixture");
        let error = view
            .config_mut()
            .copy_from(&ConfigPatch::File(FileConfigPatch {
                version: Some(1),
                producer: Some(c"x"),
                flags: Some(2),
            }))
            .unwrap_err();
        assert_kind_and_path(&error, ErrorKind::TagMismatch, &[]);
    }
    assert_unchanged(&selected, &before);
}

#[test]
fn ignored_padding_unused_capacity_and_inactive_payload_do_not_change_the_view() {
    let baseline = fixture();
    let expected = AllFeatures::access(baseline.as_bytes())
        .expect("valid baseline")
        .copy_into();

    let mut altered = fixture();
    for &(start, end) in producer::all_features_offsets::PADDING {
        altered.as_bytes_mut()[start..end].fill(0xa5);
    }
    for &(start, end) in producer::all_features_offsets::UNUSED_CAPACITY {
        altered.as_bytes_mut()[start..end].fill(0xb6);
    }
    let (start, end) = producer::all_features_offsets::INACTIVE_UNION;
    altered.as_bytes_mut()[start..end].fill(0xc7);

    let observed = AllFeatures::access(altered.as_bytes())
        .expect("ignored bytes do not participate in eager proof")
        .copy_into();
    assert_eq!(observed, expected);
}

#[test]
fn zero_sentinel_option_mut_uses_short_reborrows_and_clears_its_full_span() {
    #[repr(align(8))]
    struct Storage([u8; optional::OptionalRoot::SCHEMA_SIZE]);

    let mut storage = Storage(optional::optional_root_bytes());
    let bytes = &mut storage.0;
    let child = optional::field("maybe_child");
    let child_span = child.offset()..child.offset() + child.size();
    let parent_padding: Vec<_> = (0..optional::OptionalRoot::SCHEMA_SIZE)
        .filter(|byte| {
            !optional::OptionalRoot::LAYOUT
                .fields()
                .iter()
                .any(|field| field.offset() <= *byte && *byte < field.offset() + field.size())
        })
        .collect();
    for index in &parent_padding {
        bytes[*index] = 0x7b;
    }

    {
        let mut root = optional::OptionalRoot::access_mut(bytes)
            .expect("parent padding is excluded from proof");
        {
            let mut option = root.maybe_child_mut();
            assert!(option.get().is_none());
            option
                .set(Some(optional::Child {
                    required: optional::Required::One,
                    payload: 23,
                }))
                .expect("initialize optional child");
            {
                let mut child = option
                    .get_mut()
                    .expect("present child has a short reborrow");
                child.payload_mut().set(41).expect("nested write");
            }
            assert_eq!(option.get().expect("live child").payload(), 41);
        }
        root.maybe_kind_mut()
            .set(Some(optional::Required::One))
            .expect("initialize optional enum");
        root.maybe_array_mut()
            .set(Some([optional::Required::One, optional::Required::Two]))
            .expect("initialize optional array");
        root.maybe_tagged_mut()
            .set(Some(optional::EligibleTaggedRecord {
                required: optional::Required::One,
                tag: optional::Required::Two,
                payload: optional::Tagged::Two(optional::TaggedPayload {
                    required: optional::Required::One,
                }),
            }))
            .expect("initialize optional tagged-containing record");
    }

    bytes[child_span.start + 1] = 0xc1;
    let before_clear = *bytes;
    optional::OptionalRoot::access_mut(bytes)
        .expect("inner child padding is ignored once the child is valid")
        .maybe_child_mut()
        .set(None)
        .expect("clear optional child");
    let view =
        optional::OptionalRoot::access(bytes).expect("clearing child leaves other optionals valid");
    assert_eq!(view.maybe_kind(), Some(optional::Required::One));
    assert_eq!(
        view.maybe_array().map(|array| array.copy_into()),
        Some([optional::Required::One, optional::Required::Two])
    );
    assert_eq!(
        view.maybe_tagged()
            .expect("tagged record remains present")
            .payload()
            .two()
            .expect("selected tagged payload")
            .required(),
        optional::Required::One
    );
    assert!(bytes[child_span.clone()].iter().all(|byte| *byte == 0));
    for index in 0..bytes.len() {
        if !child_span.contains(&index) {
            assert_eq!(
                bytes[index], before_clear[index],
                "clear changed byte {index} outside the field span"
            );
        }
    }
    assert!(parent_padding.iter().all(|index| bytes[*index] == 0x7b));
}
