#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use core::{
    error::Error as _,
    ffi::CStr,
    mem::{align_of, size_of},
};

use widestring::{U16CStr, U16Str};
use zero_schema::{
    ArrayElementKind, Endian, ErrorKind, ErrorPathSegment, FieldKind, LayoutError, SchemaError,
    StringEncoding, zero,
};

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
pub struct Header<'a> {
    version: u16,
    #[zero(capacity = 6)]
    producer: &'a CStr,
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
    header: Header<'a>,
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
    header: Header<'a>,
    samples: [u32; 3],
    headers: [Header<'a>; 2],
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

fn assert_unchanged(
    bytes: &producer::AlignedAllFeatures,
    before: &[u8; producer::ALL_FEATURES_LEN],
) {
    assert_eq!(
        bytes.as_bytes(),
        before,
        "failed preflight changed wire bytes"
    );
}
fn snapshot(bytes: &producer::AlignedAllFeatures) -> [u8; producer::ALL_FEATURES_LEN] {
    bytes
        .as_bytes()
        .try_into()
        .expect("fixture length is exact")
}

#[test]
fn reviewed_fixture_has_exact_layout_and_diagnostic_metadata() {
    assert_eq!(AllFeatures::SCHEMA_SIZE, producer::ALL_FEATURES_LEN);
    assert_eq!(AllFeatures::SCHEMA_ALIGN, producer::ALL_FEATURES_ALIGN);
    assert_eq!(AllFeatures::SCHEMA_STRIDE, producer::ALL_FEATURES_LEN);
    assert_eq!(
        size_of::<producer::AlignedAllFeatures>(),
        producer::ALL_FEATURES_LEN
    );
    assert_eq!(
        align_of::<producer::AlignedAllFeatures>(),
        producer::ALL_FEATURES_ALIGN
    );

    let layout = AllFeatures::LAYOUT;
    assert_eq!(layout.name(), "AllFeatures");
    assert_eq!(
        (layout.size(), layout.align(), layout.stride()),
        (112, 16, 112)
    );
    let expected = [
        ("sequence", 0, 8, 8),
        ("active", 8, 1, 1),
        ("priority", 9, 1, 1),
        ("name", 10, 8, 1),
        ("c_name", 18, 6, 1),
        ("wide", 24, 8, 4),
        ("wide_c", 32, 6, 2),
        ("token", 38, 5, 1),
        ("header", 44, 8, 2),
        ("samples", 52, 12, 4),
        ("headers", 64, 16, 2),
        ("config_kind", 80, 1, 1),
        ("config", 84, 12, 4),
        ("checksum", 96, 1, 1),
    ];
    assert_eq!(layout.fields().len(), expected.len());
    for (index, (name, offset, size, align)) in expected.iter().enumerate() {
        let field = layout.fields()[index];
        assert_eq!(field.declaration_index(), index);
        assert_eq!(
            (field.name(), field.offset(), field.size(), field.align()),
            (*name, *offset, *size, *align)
        );
    }
    for expected_padding in [(43, 44), (81, 84), (97, 112)] {
        assert!(
            layout
                .padding()
                .iter()
                .any(|range| (range.start(), range.end()) == expected_padding)
        );
    }

    let FieldKind::String(name) = layout.fields()[3].kind() else {
        panic!("name metadata missing");
    };
    assert_eq!(name.encoding(), StringEncoding::Utf8);
    assert_eq!(name.capacity(), 7);
    assert_eq!(name.data_offset(), 1);
    assert_eq!(
        name.length().expect("name length metadata").repr(),
        zero_schema::LengthRepr::U8
    );
    assert_eq!(
        name.length().expect("name length metadata").endian(),
        Endian::Native
    );

    let FieldKind::Array(samples) = layout.fields()[9].kind() else {
        panic!("samples metadata missing");
    };
    assert_eq!(samples.length(), 3);
    assert_eq!(samples.stride(), 4);
    assert!(matches!(
        samples.element(),
        ArrayElementKind::Primitive { .. }
    ));
    let FieldKind::Array(headers) = layout.fields()[10].kind() else {
        panic!("headers metadata missing");
    };
    assert_eq!((headers.length(), headers.stride()), (2, 8));
    assert!(matches!(headers.element(), ArrayElementKind::Schema { .. }));

    let FieldKind::ExternalTaggedUnion { payload, tag } = layout.fields()[12].kind() else {
        panic!("external-union metadata missing");
    };
    assert_eq!(payload.name(), "Config");
    assert_eq!(
        (tag.field_name(), tag.offset(), tag.layout().name()),
        ("config_kind", 80, "ConfigKind")
    );
    assert_eq!(payload.variants().len(), 2);
    assert_eq!(
        (
            payload.variants()[0].name(),
            payload.variants()[0].raw_tag()
        ),
        ("File", 1)
    );
    assert_eq!(
        (
            payload.variants()[1].name(),
            payload.variants()[1].raw_tag()
        ),
        ("Memory", 2)
    );
}

#[test]
fn access_requires_an_exact_aligned_span_with_size_precedence() {
    #[repr(align(16))]
    struct Overlong([u8; 113]);

    let exact = fixture();
    assert!(AllFeatures::access(exact.as_bytes()).is_ok());

    let mut storage = Overlong([0; 113]);
    storage.0[..112].copy_from_slice(exact.as_bytes());
    let short = AllFeatures::access(&storage.0[..111]).unwrap_err();
    assert_kind_and_path(&short, ErrorKind::Layout, &[]);
    assert!(matches!(
        short
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::IncorrectSize {
            expected: 112,
            actual: 111
        })
    ));

    let extra = AllFeatures::access(&storage.0).unwrap_err();
    assert_kind_and_path(&extra, ErrorKind::Layout, &[]);
    assert!(matches!(
        extra
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::IncorrectSize {
            expected: 112,
            actual: 113
        })
    ));

    let misaligned = AllFeatures::access(&storage.0[1..]).unwrap_err();
    assert_kind_and_path(&misaligned, ErrorKind::Layout, &[]);
    assert!(matches!(
        misaligned
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::Misaligned { required: 16, .. })
    ));
}

#[test]
fn access_eagerly_proves_deterministic_failures_without_panicking() {
    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::ACTIVE] = 2;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::InvalidBool,
        &[ErrorPathSegment::Field("active")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::PRIORITY] = 99;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[ErrorPathSegment::Field("priority")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::NAME] = 8;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::LengthOutOfBounds,
        &[ErrorPathSegment::Field("name")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::NAME + 1] = 0xff;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::InvalidUtf8,
        &[ErrorPathSegment::Field("name")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::WIDE] = 3;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::LengthOutOfBounds,
        &[ErrorPathSegment::Field("wide")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()
        [producer::all_features_offsets::C_NAME..producer::all_features_offsets::WIDE]
        .fill(b'x');
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::MissingNul,
        &[ErrorPathSegment::Field("c_name")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()
        [producer::all_features_offsets::WIDE_C..producer::all_features_offsets::TOKEN]
        .fill(0x44);
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::MissingNul,
        &[ErrorPathSegment::Field("wide_c")],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[72 + 5] = b'x';
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::MissingNul,
        &[
            ErrorPathSegment::Field("headers"),
            ErrorPathSegment::Index(1),
            ErrorPathSegment::Field("producer"),
        ],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::CONFIG + 2] = 2;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::InvalidBool,
        &[
            ErrorPathSegment::Field("config"),
            ErrorPathSegment::Variant("Memory"),
            ErrorPathSegment::Field("enabled"),
        ],
    );

    let mut invalid = fixture();
    invalid.as_bytes_mut()[producer::all_features_offsets::CONFIG_KIND] =
        ConfigKind::Reserved as u8;
    let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
    assert_kind_and_path(
        &error,
        ErrorKind::UnknownUnionTag,
        &[ErrorPathSegment::Field("config")],
    );

    for offset in [0, 8, 9, 10, 18, 24, 32, 44, 64, 80, 84, 96] {
        let mut arbitrary = fixture();
        arbitrary.as_bytes_mut()[offset] = 0xff;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AllFeatures::access(arbitrary.as_bytes())
        }));
        assert!(
            outcome.is_ok(),
            "access panicked after corruption at offset {offset}"
        );
    }
}

#[test]
fn field_named_reads_are_borrowed_and_copy_into_is_equivalent() {
    let bytes = fixture();
    let view = AllFeatures::access(bytes.as_bytes()).unwrap();
    assert_eq!(view.sequence(), 0x0707_0707_0707_0707);
    assert!(view.active());
    assert_eq!(view.priority(), Priority::High);
    assert_eq!(view.name(), "api");
    assert_eq!(view.c_name().to_bytes(), b"svc");
    assert_eq!(view.wide().as_slice(), &[0x4141]);
    assert_eq!(view.wide_c().as_slice(), &[0x4444]);
    assert_eq!(view.token(), &[0x10, 0x20, 0x30, 0x40, 0x50]);
    assert_eq!(
        (view.header().version(), view.header().producer().to_bytes()),
        (0x2222, b"prod".as_slice())
    );
    assert_eq!(view.samples().get(1), Some(0x1212_1212));
    assert_eq!(
        view.samples().iter().collect::<Vec<_>>(),
        vec![0x1111_1111, 0x1212_1212, 0x1313_1313]
    );
    assert_eq!(
        view.samples().copy_into(),
        [0x1111_1111, 0x1212_1212, 0x1313_1313]
    );
    assert_eq!(
        view.headers()
            .iter()
            .map(|header| header.producer().to_bytes())
            .collect::<Vec<_>>(),
        vec![b"one".as_slice(), b"two".as_slice()]
    );
    assert_eq!(view.headers().copy_into()[0].version(), 0x2424);
    assert_eq!(view.config_kind(), ConfigKind::Memory);
    let config = view.config();
    assert_eq!(config.tag(), ConfigKind::Memory);
    assert!(config.file().is_none());
    let memory = config.memory().expect("selected Memory payload");
    assert_eq!((memory.capacity(), memory.enabled()), (0x3333, true));
    assert!(matches!(
        config.copy_into(),
        Config::Memory(MemoryConfig {
            capacity: 0x3333,
            enabled: true
        })
    ));
    assert_eq!(view.checksum(), 0x6a);

    let start = bytes.as_bytes().as_ptr() as usize;
    let end = start + bytes.as_bytes().len();
    for (pointer, length) in [
        (view.name().as_ptr() as usize, view.name().len()),
        (
            view.c_name().as_ptr() as usize,
            view.c_name().to_bytes_with_nul().len(),
        ),
        (view.wide().as_ptr() as usize, size_of::<u16>()),
        (view.wide_c().as_ptr() as usize, size_of::<u16>() * 2),
        (view.token().as_ptr() as usize, view.token().len()),
    ] {
        assert!(
            pointer >= start && pointer + length <= end,
            "getter must borrow fixture bytes"
        );
    }

    let copied = view.copy_into();
    assert_eq!(copied.sequence, view.sequence());
    assert_eq!(copied.name, view.name());
    assert_eq!(
        copied.header.producer.to_bytes(),
        view.header().producer().to_bytes()
    );
    assert_eq!(copied.samples, view.samples().copy_into());
    assert!(matches!(
        copied.config,
        Config::Memory(MemoryConfig {
            capacity: 0x3333,
            enabled: true
        })
    ));
}

#[test]
fn field_local_mutation_reaccesses_and_uses_short_reborrows() {
    let mut bytes = fixture();
    {
        let mut view = AllFeatures::access_mut(bytes.as_bytes_mut()).unwrap();
        let before = view.name().as_ptr();
        view.sequence_mut().set(43).unwrap();
        view.active_mut().set(false).unwrap();
        view.priority_mut().set(Priority::Normal).unwrap();
        view.name_mut().set("zero").unwrap();
        view.c_name_mut().set(c"c").unwrap();
        let wide = U16Str::from_slice(&[0x5151, 0x5252]);
        let wide_c = U16CStr::from_slice(&[0x6161, 0]).unwrap();
        view.wide_mut().set(wide).unwrap();
        view.wide_c_mut().set(wide_c).unwrap();
        view.token_mut().set(b"abcde").unwrap();
        {
            let mut header = view.header_mut();
            header.version_mut().set(0x4444).unwrap();
            header.producer_mut().set(c"hdr").unwrap();
        }
        {
            let mut samples = view.samples_mut();
            samples.get_mut(1).unwrap().set(21).unwrap();
            samples.set(0, 19).unwrap();
            samples.copy_from(&[19, 21, 23]).unwrap();
            assert_eq!(samples.copy_into(), [19, 21, 23]);
        }
        {
            let mut headers = view.headers_mut();
            let mut first = headers.get_mut(0).unwrap();
            first.version_mut().set(0x6666).unwrap();
            headers
                .copy_from(&[
                    Header {
                        version: 0x6666,
                        producer: c"one",
                    },
                    Header {
                        version: 0x7777,
                        producer: c"two",
                    },
                ])
                .unwrap();
            assert_eq!(headers.copy_into()[1].version(), 0x7777);
        }
        {
            let mut config = view.config_mut();
            let mut memory = config.memory_mut().expect("Memory remains selected");
            memory.capacity_mut().set(0x7777).unwrap();
            memory.enabled_mut().set(false).unwrap();
            assert!(matches!(
                config.copy_into(),
                Config::Memory(MemoryConfig {
                    capacity: 0x7777,
                    enabled: false
                })
            ));
        }
        let copied = view.copy_into();
        assert_eq!(copied.sequence, 43);
        assert_eq!(copied.samples, [19, 21, 23]);
        assert!(matches!(
            copied.config,
            Config::Memory(MemoryConfig {
                capacity: 0x7777,
                enabled: false
            })
        ));
        assert_ne!(view.name().as_ptr(), core::ptr::null());
        assert_eq!(
            before,
            view.name().as_ptr(),
            "getter reborrows the same producer storage"
        );
    }

    let view = AllFeatures::access(bytes.as_bytes()).unwrap();
    assert_eq!(
        (view.sequence(), view.active(), view.priority(), view.name()),
        (43, false, Priority::Normal, "zero")
    );
    assert_eq!(view.c_name().to_bytes(), b"c");
    assert_eq!(view.wide().as_slice(), &[0x5151, 0x5252]);
    assert_eq!(view.wide_c().as_slice(), &[0x6161]);
    assert_eq!(view.token(), b"abcde");
    assert_eq!(
        (view.header().version(), view.header().producer().to_bytes()),
        (0x4444, b"hdr".as_slice())
    );
    assert_eq!(view.samples().copy_into(), [19, 21, 23]);
    assert_eq!(view.headers().copy_into()[1].version(), 0x7777);
    assert_eq!(
        (
            view.config().memory().unwrap().capacity(),
            view.config().memory().unwrap().enabled()
        ),
        (0x7777, false)
    );
}

#[test]
fn patches_cover_noop_nested_arrays_and_all_external_tag_cases() {
    let mut no_op = fixture();
    let before = snapshot(&no_op);
    {
        let mut view = AllFeatures::access_mut(no_op.as_bytes_mut()).unwrap();
        view.copy_from(&AllFeaturesPatch::default()).unwrap();
    }
    assert_unchanged(&no_op, &before);

    let mut partial = fixture();
    {
        let mut view = AllFeatures::access_mut(partial.as_bytes_mut()).unwrap();
        view.copy_from(&AllFeaturesPatch {
            header: Some(HeaderPatch {
                version: Some(0x8888),
                producer: None,
            }),
            ..Default::default()
        })
        .unwrap();
    }
    let partial_view = AllFeatures::access(partial.as_bytes()).unwrap();
    assert_eq!(partial_view.header().version(), 0x8888);
    assert_eq!(partial_view.header().producer().to_bytes(), b"prod");

    let mut array = fixture();
    {
        let mut view = AllFeatures::access_mut(array.as_bytes_mut()).unwrap();
        view.copy_from(&AllFeaturesPatch {
            samples: Some([31, 37, 41]),
            ..Default::default()
        })
        .unwrap();
    }
    assert_eq!(
        AllFeatures::access(array.as_bytes())
            .unwrap()
            .samples()
            .copy_into(),
        [31, 37, 41]
    );

    let source = fixture();
    let logical = AllFeatures::access(source.as_bytes()).unwrap().copy_into();
    let patch = AllFeaturesPatch::from(logical);
    let mut derived = fixture();
    AllFeatures::access_mut(derived.as_bytes_mut())
        .unwrap()
        .copy_from(&patch)
        .unwrap();
    assert!(AllFeatures::access(derived.as_bytes()).is_ok());

    let mut tag_only = fixture();
    let before = snapshot(&tag_only);
    {
        let mut view = AllFeatures::access_mut(tag_only.as_bytes_mut()).unwrap();
        let patch = AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: None,
            ..Default::default()
        };
        let error = view.copy_from(&patch).unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::TagOnlyPatch,
            &[ErrorPathSegment::Field("config")],
        );
    }
    assert_unchanged(&tag_only, &before);

    let incomplete_file = ConfigPatch::File(FileConfigPatch {
        header: Some(HeaderPatch {
            version: Some(7),
            producer: None,
        }),
        flags: Some(9),
    });
    let mut incomplete = fixture();
    let before = snapshot(&incomplete);
    {
        let mut view = AllFeatures::access_mut(incomplete.as_bytes_mut()).unwrap();
        let patch = AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: Some(incomplete_file),
            ..Default::default()
        };
        let error = view.copy_from(&patch).unwrap_err();
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
        let mut view = AllFeatures::access_mut(mismatch.as_bytes_mut()).unwrap();
        let patch = AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: Some(ConfigPatch::Memory(MemoryConfigPatch {
                capacity: Some(9),
                enabled: Some(false),
            })),
            ..Default::default()
        };
        let error = view.copy_from(&patch).unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::TagMismatch,
            &[ErrorPathSegment::Field("config")],
        );
    }
    assert_unchanged(&mismatch, &before);

    let mut same_variant = fixture();
    {
        let mut view = AllFeatures::access_mut(same_variant.as_bytes_mut()).unwrap();
        let patch = AllFeaturesPatch {
            config_kind: Some(ConfigKind::Memory),
            config: Some(ConfigPatch::Memory(MemoryConfigPatch {
                capacity: Some(0x9999),
                enabled: None,
            })),
            ..Default::default()
        };
        view.copy_from(&patch).unwrap();
    }
    let view = AllFeatures::access(same_variant.as_bytes()).unwrap();
    assert_eq!(view.config_kind(), ConfigKind::Memory);
    assert_eq!(view.config().memory().unwrap().capacity(), 0x9999);
    assert!(view.config().memory().unwrap().enabled());

    let full_file = ConfigPatch::File(FileConfigPatch {
        header: Some(HeaderPatch {
            version: Some(0x9999),
            producer: Some(c"file"),
        }),
        flags: Some(0x0102_0304),
    });
    let mut derived_tag = fixture();
    {
        let mut view = AllFeatures::access_mut(derived_tag.as_bytes_mut()).unwrap();
        let patch = AllFeaturesPatch {
            config_kind: None,
            config: Some(full_file),
            ..Default::default()
        };
        view.copy_from(&patch).unwrap();
    }
    let view = AllFeatures::access(derived_tag.as_bytes()).unwrap();
    assert_eq!(view.config_kind(), ConfigKind::File);
    let file = view.config().file().expect("union patch derives File tag");
    assert_eq!(
        (
            file.header().version(),
            file.header().producer().to_bytes(),
            file.flags()
        ),
        (0x9999, b"file".as_slice(), 0x0102_0304)
    );
    assert!(view.config().memory().is_none());
}

#[test]
fn every_failed_source_or_preflight_preserves_the_entire_fixture() {
    let mut string = fixture();
    let before = snapshot(&string);
    {
        let mut view = AllFeatures::access_mut(string.as_bytes_mut()).unwrap();
        let error = view.name_mut().set("overlong").unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::CapacityExceeded,
            &[ErrorPathSegment::Field("name")],
        );
    }
    assert_unchanged(&string, &before);

    let mut bytes = fixture();
    let before = snapshot(&bytes);
    {
        let mut view = AllFeatures::access_mut(bytes.as_bytes_mut()).unwrap();
        let error = view.token_mut().set(b"tiny").unwrap_err();
        assert_kind_and_path(
            &error,
            ErrorKind::ArrayLengthMismatch,
            &[ErrorPathSegment::Field("token")],
        );
    }
    assert_unchanged(&bytes, &before);

    let mut index = fixture();
    let before = snapshot(&index);
    {
        let mut view = AllFeatures::access_mut(index.as_bytes_mut()).unwrap();
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
        let mut view = AllFeatures::access_mut(length.as_bytes_mut()).unwrap();
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
        let mut view = AllFeatures::access_mut(selected.as_bytes_mut()).unwrap();
        let mut config = view.config_mut();
        let error = config
            .copy_from(&ConfigPatch::File(FileConfigPatch {
                header: Some(HeaderPatch {
                    version: Some(1),
                    producer: Some(c"x"),
                }),
                flags: Some(2),
            }))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TagMismatch);
    }
    assert_unchanged(&selected, &before);
}

#[test]
fn ignored_padding_capacity_and_inactive_union_bytes_do_not_affect_access() {
    let baseline = fixture();
    let expected = AllFeatures::access(baseline.as_bytes())
        .unwrap()
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
        .expect("ignored bytes do not participate in proof")
        .copy_into();
    assert_eq!(observed, expected);
}
