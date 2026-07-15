#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use core::mem::{align_of, size_of};

use capabilities::{AllFeatures, ConfigKind, Priority};
use widestring::{U16CStr, U16Str};
use zero_schema::{ArrayElementKind, Endian, FieldKind, LengthRepr, StringEncoding};

#[test]
fn producer_bytes_expose_borrowed_capabilities_and_layout_metadata() {
    let fixture = producer::all_features_mut();
    assert!(fixture.is_exactly_aligned());
    assert_eq!(AllFeatures::SCHEMA_SIZE, producer::ALL_FEATURES_LEN);
    assert_eq!(AllFeatures::SCHEMA_ALIGN, producer::ALL_FEATURES_ALIGN);
    assert_eq!(AllFeatures::SCHEMA_STRIDE, producer::ALL_FEATURES_LEN);
    assert_eq!(
        size_of::<producer::AlignedAllFeatures>(),
        AllFeatures::SCHEMA_SIZE
    );
    assert_eq!(
        align_of::<producer::AlignedAllFeatures>(),
        AllFeatures::SCHEMA_ALIGN
    );

    let layout = AllFeatures::LAYOUT;
    assert_eq!(
        (
            layout.name(),
            layout.size(),
            layout.align(),
            layout.stride()
        ),
        ("AllFeatures", 112, 16, 112)
    );
    let expected = [
        ("sequence", 0),
        ("active", 8),
        ("priority", 9),
        ("name", 10),
        ("c_name", 18),
        ("wide", 24),
        ("wide_c", 32),
        ("token", 38),
        ("header", 44),
        ("samples", 52),
        ("headers", 64),
        ("config_kind", 80),
        ("config", 84),
        ("checksum", 96),
    ];
    assert_eq!(layout.fields().len(), expected.len());
    for (index, (name, offset)) in expected.iter().enumerate() {
        let field = layout.fields()[index];
        assert_eq!(
            (field.declaration_index(), field.name(), field.offset()),
            (index, *name, *offset)
        );
        assert_eq!(field.offset() % field.align(), 0);
    }
    assert!(
        layout
            .padding()
            .iter()
            .any(|range| (range.start(), range.end()) == (97, 112))
    );
    let FieldKind::String(name) = layout.fields()[3].kind() else {
        panic!("name must be string metadata");
    };
    assert_eq!(
        (name.encoding(), name.capacity(), name.data_offset()),
        (StringEncoding::Utf8, 7, 1)
    );
    assert_eq!(
        (
            name.length().unwrap().repr(),
            name.length().unwrap().endian()
        ),
        (LengthRepr::U8, Endian::Native)
    );
    let FieldKind::Array(samples) = layout.fields()[9].kind() else {
        panic!("samples must be array metadata");
    };
    assert_eq!((samples.length(), samples.stride()), (3, 4));
    assert!(matches!(
        samples.element(),
        ArrayElementKind::Primitive { .. }
    ));

    let view = AllFeatures::access(fixture.as_bytes()).expect("reviewed producer bytes are valid");
    assert_eq!(
        (view.sequence(), view.active(), view.priority(), view.name()),
        (0x0707_0707_0707_0707, true, Priority::High, "api")
    );
    assert_eq!(view.c_name().to_bytes(), b"svc");
    assert_eq!(view.wide().as_slice(), &[0x4141]);
    assert_eq!(view.wide_c().as_slice(), &[0x4444]);
    assert_eq!(view.token(), &[0x10, 0x20, 0x30, 0x40, 0x50]);
    assert_eq!(
        (view.header().version(), view.header().producer().to_bytes()),
        (0x2222, b"prod".as_slice())
    );
    assert_eq!(
        view.samples().copy_into(),
        [0x1111_1111, 0x1212_1212, 0x1313_1313]
    );
    assert_eq!(view.headers().get(1).unwrap().producer().to_bytes(), b"two");
    assert_eq!(view.config_kind(), ConfigKind::Memory);
    assert_eq!(view.config().tag(), ConfigKind::Memory);
    assert_eq!(
        (
            view.config().memory().unwrap().capacity(),
            view.config().memory().unwrap().enabled()
        ),
        (0x3333, true)
    );

    let start = fixture.as_bytes().as_ptr() as usize;
    let end = start + fixture.as_bytes().len();
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
            "read capability must borrow producer storage"
        );
    }

    let copied = view.copy_into();
    assert_eq!(copied.sequence, view.sequence());
    assert_eq!(copied.samples, view.samples().copy_into());
    assert_eq!(
        copied.header.producer.to_bytes(),
        view.header().producer().to_bytes()
    );
}

#[test]
fn short_mutable_reborrows_update_only_constrained_fields() {
    let mut fixture = producer::all_features_mut();
    {
        let mut view = AllFeatures::access_mut(fixture.as_bytes_mut()).unwrap();
        view.sequence_mut().set(43).unwrap();
        view.name_mut().set("zero").unwrap();
        view.c_name_mut().set(c"c").unwrap();
        view.wide_mut()
            .set(U16Str::from_slice(&[0x5151, 0x5252]))
            .unwrap();
        view.wide_c_mut()
            .set(U16CStr::from_slice(&[0x6161, 0]).unwrap())
            .unwrap();
        {
            let mut header = view.header_mut();
            header.version_mut().set(0x4444).unwrap();
            header.producer_mut().set(c"hdr").unwrap();
        }
        {
            let mut samples = view.samples_mut();
            samples.get_mut(1).unwrap().set(21).unwrap();
            samples.copy_from(&[19, 21, 23]).unwrap();
        }
    }
    let refreshed = AllFeatures::access(fixture.as_bytes()).unwrap();
    assert_eq!(
        (
            refreshed.sequence(),
            refreshed.name(),
            refreshed.c_name().to_bytes()
        ),
        (43, "zero", b"c".as_slice())
    );
    assert_eq!(refreshed.wide().as_slice(), &[0x5151, 0x5252]);
    assert_eq!(refreshed.wide_c().as_slice(), &[0x6161]);
    assert_eq!(
        (
            refreshed.header().version(),
            refreshed.header().producer().to_bytes()
        ),
        (0x4444, b"hdr".as_slice())
    );
    assert_eq!(refreshed.samples().copy_into(), [19, 21, 23]);
}
