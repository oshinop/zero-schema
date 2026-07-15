#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{
    AllFeatures, AllFeaturesPatch, ConfigKind, ConfigPatch, FileConfigPatch, HeaderPatch,
    MemoryConfigPatch,
};
use zero_schema::{ErrorKind, SchemaError};

fn snapshot(bytes: &producer::AlignedAllFeatures) -> [u8; producer::ALL_FEATURES_LEN] {
    bytes.as_bytes().try_into().unwrap()
}

#[test]
fn external_tag_selects_one_payload_and_same_variant_patch_is_constrained() {
    let mut fixture = producer::all_features_mut();
    {
        let mut view = AllFeatures::access_mut(fixture.as_bytes_mut()).unwrap();
        let mut config = view.config_mut();
        assert_eq!(config.tag(), ConfigKind::Memory);
        assert!(config.file_mut().is_none());
        config
            .memory_mut()
            .unwrap()
            .capacity_mut()
            .set(0x9999)
            .unwrap();
    }
    let view = AllFeatures::access(fixture.as_bytes()).unwrap();
    assert_eq!(
        (
            view.config().tag(),
            view.config().memory().unwrap().capacity()
        ),
        (ConfigKind::Memory, 0x9999)
    );
}

#[test]
fn external_union_patch_matrix_derives_tag_and_rejects_incomplete_or_mismatched_switches() {
    let mut incomplete = producer::all_features_mut();
    let before = snapshot(&incomplete);
    let error = AllFeatures::access_mut(incomplete.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: Some(ConfigPatch::File(FileConfigPatch {
                header: Some(HeaderPatch {
                    version: Some(7),
                    producer: None,
                }),
                flags: Some(9),
            })),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::IncompleteUnionSwitch);
    assert_eq!(incomplete.as_bytes(), before);

    let mut mismatch = producer::all_features_mut();
    let before = snapshot(&mismatch);
    let error = AllFeatures::access_mut(mismatch.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch {
            config_kind: Some(ConfigKind::File),
            config: Some(ConfigPatch::Memory(MemoryConfigPatch {
                capacity: Some(9),
                enabled: Some(false),
            })),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TagMismatch);
    assert_eq!(mismatch.as_bytes(), before);

    let mut switched = producer::all_features_mut();
    AllFeatures::access_mut(switched.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch {
            config_kind: None,
            config: Some(ConfigPatch::File(FileConfigPatch {
                header: Some(HeaderPatch {
                    version: Some(0x9999),
                    producer: Some(c"file"),
                }),
                flags: Some(0x0102_0304),
            })),
            ..Default::default()
        })
        .unwrap();
    let view = AllFeatures::access(switched.as_bytes()).unwrap();
    let file = view.config().file().unwrap();
    assert_eq!(
        (
            view.config().tag(),
            file.header().producer().to_bytes(),
            file.flags()
        ),
        (ConfigKind::File, b"file".as_slice(), 0x0102_0304)
    );
}
