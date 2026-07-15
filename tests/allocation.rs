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

use core::fmt::{self, Write as _};

use capabilities::{AllFeatures, AllFeaturesPatch};
use counting_alloc::{assert_instrumentation_works, zero_allocations};
use optional::{Child, ChildPatch, OptionalRoot, OptionalRootPatch, Required, optional_root_bytes};

struct FixedText {
    bytes: [u8; 256],
    length: usize,
}

impl FixedText {
    const fn new() -> Self {
        Self {
            bytes: [0; 256],
            length: 0,
        }
    }
}

impl fmt::Write for FixedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}

#[test]
fn access_reads_copies_arrays_unions_patches_and_error_formatting_allocate_nothing() {
    assert_instrumentation_works();
    let source = producer::all_features_mut();
    let mut destination = producer::all_features_mut();
    let mut invalid = producer::all_features_mut();
    invalid.as_bytes_mut()[producer::all_features_offsets::ACTIVE] = 2;

    zero_allocations(|| {
        let view = AllFeatures::access(source.as_bytes()).unwrap();
        let logical = view.copy_into();
        assert_eq!(view.samples().copy_into(), logical.samples);
        assert_eq!(view.config().copy_into(), logical.config);
        AllFeatures::access_mut(destination.as_bytes_mut())
            .unwrap()
            .copy_from(&AllFeaturesPatch::from(logical))
            .unwrap();
        assert_eq!(
            AllFeatures::access(destination.as_bytes()).unwrap().name(),
            "api"
        );

        let error = AllFeatures::access(invalid.as_bytes()).unwrap_err();
        let mut text = FixedText::new();
        write!(&mut text, "{error}").unwrap();
        assert!(text.length > 0);
    });
}

#[test]
fn zero_sentinel_public_operations_allocate_nothing() {
    let mut bytes = optional_root_bytes();
    zero_allocations(|| {
        {
            let mut root =
                OptionalRoot::access_mut(&mut bytes).expect("absent optionals are valid");
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
