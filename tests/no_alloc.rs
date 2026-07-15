#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/counting_alloc.rs"]
mod counting_alloc;
#[path = "support/optional.rs"]
#[allow(dead_code)]
mod optional;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use capabilities::{AllFeatures, AllFeaturesPatch};
use counting_alloc::{assert_instrumentation_works, zero_allocations};
use optional::{
    Child, ChildPatch, EligibleTaggedRecord, OptionalRoot, OptionalRootPatch, Required, Tagged,
    TaggedPayload, optional_root_bytes,
};

#[test]
fn core_capability_operations_remain_allocation_free_on_reviewed_producer_bytes() {
    assert_instrumentation_works();
    let mut bytes = producer::all_features_mut();
    zero_allocations(|| {
        let mut view = AllFeatures::access_mut(bytes.as_bytes_mut()).unwrap();
        assert_eq!(view.samples().get(2), Some(0x1313_1313));
        view.samples_mut().copy_from(&[3, 5, 8]).unwrap();
        view.copy_from(&AllFeaturesPatch::default()).unwrap();
    });
    zero_allocations(|| {
        let view = AllFeatures::access(bytes.as_bytes()).unwrap();
        assert_eq!(view.samples().copy_into(), [3, 5, 8]);
        let _ = view.copy_into();
    });
}

#[test]
fn zero_sentinel_access_copy_option_mut_and_patch_remain_allocation_free() {
    let mut bytes = optional_root_bytes();
    zero_allocations(|| {
        {
            let mut root =
                OptionalRoot::access_mut(&mut bytes).expect("absent optionals are valid");
            {
                let mut child = root.maybe_child_mut();
                child
                    .set(Some(Child {
                        required: Required::One,
                        payload: 23,
                    }))
                    .expect("initialize optional child");
                {
                    let mut nested = child.get_mut().expect("child is present");
                    nested.payload_mut().set(41).expect("nested mutation");
                }
                assert_eq!(child.get().expect("live child").payload(), 41);
            }
            root.maybe_kind_mut()
                .set(Some(Required::One))
                .expect("initialize optional enum");
            root.maybe_array_mut()
                .set(Some([Required::One, Required::Two]))
                .expect("initialize optional array");
            root.maybe_tagged_mut()
                .set(Some(EligibleTaggedRecord {
                    required: Required::One,
                    tag: Required::Two,
                    payload: Tagged::Two(TaggedPayload {
                        required: Required::One,
                    }),
                }))
                .expect("initialize optional tagged-containing record");
        }
        for (payload, required) in [
            (41, Required::Two),
            (43, Required::One),
            (47, Required::Two),
            (53, Required::One),
        ] {
            {
                let mut root =
                    OptionalRoot::access_mut(&mut bytes).expect("present child is valid");
                root.maybe_child_mut()
                    .get_mut()
                    .expect("child remains present")
                    .payload_mut()
                    .set(payload)
                    .expect("repeated OptionMut nested mutation");
            }
            OptionalRoot::access_mut(&mut bytes)
                .expect("present child is valid")
                .copy_from(&OptionalRootPatch {
                    maybe_child: Some(Some(ChildPatch {
                        required: Some(required.into()),
                        payload: None,
                    })),
                    ..Default::default()
                })
                .expect("repeated present partial optional patch");
            let view = OptionalRoot::access(&bytes).expect("patched bytes are valid");
            assert_eq!(view.maybe_kind(), Some(Required::One));
            assert_eq!(
                view.maybe_array().map(|array| array.copy_into()),
                Some([Required::One, Required::Two])
            );
            assert_eq!(
                view.maybe_tagged()
                    .expect("tagged record remains present")
                    .payload()
                    .two()
                    .expect("selected tagged payload")
                    .required(),
                Required::One
            );
            assert_eq!(
                view.maybe_child()
                    .expect("child remains present")
                    .required(),
                required
            );
            assert_eq!(
                view.copy_into()
                    .maybe_child
                    .expect("materialized child")
                    .payload,
                payload
            );
        }
    });
}
