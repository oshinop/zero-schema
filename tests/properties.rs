#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, AllFeaturesPatch};

const CASES: usize = 256;
const SEED: u64 = 0x5a53_3031_5f50_524f;

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn aligned_arbitrary_initialized_storage_never_panics_during_eager_proof() {
    let mut state = SEED;
    for case in 0..CASES {
        let mut bytes = producer::all_features_mut();
        for byte in bytes.as_bytes_mut() {
            *byte = next(&mut state) as u8;
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AllFeatures::access(bytes.as_bytes())
        }));
        assert!(
            outcome.is_ok(),
            "access panicked for deterministic arbitrary case {case}"
        );
        if let Ok(view) = outcome.unwrap() {
            let _ = view.sequence();
            let _ = view.samples().copy_into();
            let _ = view.config().copy_into();
            let _ = view.copy_into();
        }
    }
}

#[test]
fn producer_bytes_survive_noop_patch_and_reaccess_for_every_deterministic_clone() {
    for case in 0..CASES {
        let mut bytes = producer::all_features_mut();
        let before: [u8; producer::ALL_FEATURES_LEN] = bytes.as_bytes().try_into().unwrap();
        AllFeatures::access_mut(bytes.as_bytes_mut())
            .unwrap()
            .copy_from(&AllFeaturesPatch::default())
            .unwrap();
        assert_eq!(
            bytes.as_bytes(),
            before,
            "no-op patch changed producer clone {case}"
        );
        assert!(AllFeatures::access(bytes.as_bytes()).is_ok());
    }
}
