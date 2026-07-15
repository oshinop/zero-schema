#[path = "support/producer.rs"]
mod producer;

use sha2::{Digest, Sha256};

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn reviewed_all_features_fixture_is_independent_aligned_storage() {
    let source = producer::all_features();
    assert_eq!(source.len(), producer::ALL_FEATURES_LEN);
    assert_eq!(hex(Sha256::digest(source)), producer::ALL_FEATURES_SHA256);

    let mut clone = producer::all_features_mut();
    assert!(clone.is_exactly_aligned());
    assert_eq!(clone.as_bytes(), source);
    clone.as_bytes_mut()[0] ^= 0xff;
    assert_ne!(clone.as_bytes(), source);
    assert_eq!(producer::all_features(), source);
}

#[test]
fn reviewed_fixture_contains_nonzero_ignored_storage() {
    let bytes = producer::all_features();

    for &(start, end) in producer::all_features_offsets::PADDING {
        assert!(
            bytes[start..end].iter().all(|byte| *byte != 0),
            "padding range {start}..{end} must stay nonzero"
        );
    }
    for &(start, end) in producer::all_features_offsets::UNUSED_CAPACITY {
        assert!(
            bytes[start..end].iter().all(|byte| *byte != 0),
            "unused capacity {start}..{end} must stay nonzero"
        );
    }
    let (start, end) = producer::all_features_offsets::INACTIVE_UNION;
    assert_ne!(&bytes[start..end], &[0; 8]);

    let offsets = [
        producer::all_features_offsets::SEQUENCE,
        producer::all_features_offsets::ACTIVE,
        producer::all_features_offsets::PRIORITY,
        producer::all_features_offsets::NAME,
        producer::all_features_offsets::C_NAME,
        producer::all_features_offsets::WIDE,
        producer::all_features_offsets::WIDE_C,
        producer::all_features_offsets::TOKEN,
        producer::all_features_offsets::HEADER,
        producer::all_features_offsets::SAMPLES,
        producer::all_features_offsets::HEADERS,
        producer::all_features_offsets::CONFIG_KIND,
        producer::all_features_offsets::CONFIG,
        producer::all_features_offsets::CHECKSUM,
    ];
    assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
}
