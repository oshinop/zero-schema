#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, AllFeaturesPatch, Config, ConfigKind, HeaderPatch, MemoryConfig};

#[test]
fn producer_access_materialize_patch_and_fresh_access_form_the_transfer_contract() {
    let source = producer::all_features_mut();
    let logical = AllFeatures::access(source.as_bytes()).unwrap().copy_into();
    assert!(matches!(
        logical.config,
        Config::Memory(MemoryConfig {
            capacity: 0x3333,
            enabled: true
        })
    ));

    let mut destination = producer::all_features_mut();
    AllFeatures::access_mut(destination.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch::from(logical))
        .unwrap();
    let copied = AllFeatures::access(destination.as_bytes()).unwrap();
    assert_eq!(
        (
            copied.sequence(),
            copied.name(),
            copied.samples().copy_into()
        ),
        (
            0x0707_0707_0707_0707,
            "api",
            [0x1111_1111, 0x1212_1212, 0x1313_1313]
        )
    );
    assert_eq!(copied.config().tag(), ConfigKind::Memory);

    let mut partial = producer::all_features_mut();
    AllFeatures::access_mut(partial.as_bytes_mut())
        .unwrap()
        .copy_from(&AllFeaturesPatch {
            header: Some(HeaderPatch {
                version: Some(0x8888),
                producer: None,
            }),
            samples: Some([31, 37, 41]),
            ..Default::default()
        })
        .unwrap();
    let refreshed = AllFeatures::access(partial.as_bytes()).unwrap();
    assert_eq!(
        (
            refreshed.header().version(),
            refreshed.header().producer().to_bytes()
        ),
        (0x8888, b"prod".as_slice())
    );
    assert_eq!(refreshed.samples().copy_into(), [31, 37, 41]);
}
