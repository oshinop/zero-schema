#![deny(warnings)]

#[path = "support/optional.rs"]
mod optional;

use optional::{
    Child, ChildPatch, EligibleTaggedRecord, EligibleTextChild, EligibleTextChildPatch,
    OptionalRoot, OptionalRootPatch, PaddedEligibleChild, PaddedOptionalArrayRoot, Required,
    Tagged, TaggedPayload, field, optional_root_bytes, padded_array_bytes, padded_array_field,
};
use zero_schema::{ErrorKind, ErrorPathSegment, FieldKind, SchemaError, error_path_string};

fn field_span(name: &str) -> core::ops::Range<usize> {
    let field = field(name);
    field.offset()..field.offset() + field.size()
}

fn root_padding_indices() -> impl Iterator<Item = usize> {
    (0..OptionalRoot::SCHEMA_SIZE).filter(|byte| {
        !OptionalRoot::LAYOUT
            .fields()
            .iter()
            .any(|field| field.offset() <= *byte && *byte < field.offset() + field.size())
    })
}

fn assert_path(error: &impl SchemaError, kind: ErrorKind, path: &[ErrorPathSegment]) {
    assert_eq!(error.kind(), kind, "unexpected error: {error}");
    let mut current: &dyn SchemaError = error;
    for (index, segment) in path.iter().enumerate() {
        assert_eq!(
            current.segment(),
            Some(*segment),
            "unexpected path: {error}"
        );
        if index + 1 < path.len() {
            current = current.child().expect("missing nested error segment");
        }
    }
}

#[test]
fn option_layout_uses_the_inner_field_kind_and_complete_storage_span() {
    let fields = OptionalRoot::LAYOUT.fields();
    let expected = [
        ("before", 0, 1, 1, false),
        ("maybe_kind", 8, 8, 8, true),
        ("maybe_child", 16, 8, 4, true),
        ("maybe_array", 24, 2, 1, true),
        ("maybe_text", 26, 6, 1, true),
        ("maybe_tagged", 32, 3, 1, true),
        ("after", 35, 1, 1, false),
    ];

    assert_eq!(
        (OptionalRoot::SCHEMA_SIZE, OptionalRoot::SCHEMA_ALIGN),
        (40, 8)
    );
    assert_eq!(fields.len(), expected.len());
    for (descriptor, (name, offset, size, align, optional)) in fields.iter().zip(expected) {
        assert_eq!(
            (
                descriptor.name(),
                descriptor.offset(),
                descriptor.size(),
                descriptor.align(),
                descriptor.is_optional(),
            ),
            (name, offset, size, align, optional)
        );
    }

    assert!(matches!(
        field("maybe_kind").kind(),
        FieldKind::ScalarEnum { .. }
    ));
    assert!(matches!(
        field("maybe_child").kind(),
        FieldKind::Schema { .. }
    ));
    let FieldKind::Array(array) = field("maybe_array").kind() else {
        panic!("optional fixed array must expose its inner array kind");
    };
    assert_eq!((array.length(), array.stride()), (2, 1));
    assert!(matches!(
        field("maybe_text").kind(),
        FieldKind::Schema { .. }
    ));
    assert!(matches!(
        field("maybe_tagged").kind(),
        FieldKind::Schema { .. }
    ));

    assert_eq!(field("maybe_child").size(), Child::SCHEMA_SIZE);
    assert_eq!(field("maybe_array").size(), 2);
    assert_eq!(field("maybe_kind").size(), field("maybe_kind").align());
    assert_eq!(
        field("maybe_kind").size(),
        8,
        "the requested field alignment, not an Option presence byte, owns this span"
    );
}

#[test]
fn zero_sentinel_access_materializes_only_valid_present_values() {
    let mut bytes = optional_root_bytes();
    let parent_padding: Vec<_> = root_padding_indices().collect();
    assert!(
        !parent_padding.is_empty(),
        "root intentionally has excluded padding"
    );
    for index in &parent_padding {
        bytes[*index] = 0xa5;
    }

    let absent = OptionalRoot::access(&bytes).expect("all-zero complete option spans are absent");
    assert!(absent.maybe_kind().is_none());
    assert!(absent.maybe_child().is_none());
    assert!(absent.maybe_array().is_none());
    assert!(absent.maybe_text().is_none());
    assert!(absent.maybe_tagged().is_none());

    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("parent padding is excluded");
        root.maybe_kind_mut()
            .set(Some(Required::One))
            .expect("valid enum initializes optional");
        root.maybe_child_mut()
            .set(Some(Child {
                required: Required::Two,
                payload: 23,
            }))
            .expect("valid child initializes optional");
        root.maybe_array_mut()
            .set(Some([Required::One, Required::Two]))
            .expect("valid array initializes optional");
    }

    let logical = OptionalRoot::access(&bytes)
        .expect("valid nonzero option spans materialize")
        .copy_into();
    assert_eq!(logical.maybe_kind, Some(Required::One));
    assert_eq!(
        logical.maybe_child,
        Some(Child {
            required: Required::Two,
            payload: 23,
        })
    );
    assert_eq!(logical.maybe_array, Some([Required::One, Required::Two]));
    assert!(parent_padding.iter().all(|index| bytes[*index] == 0xa5));
}

#[test]
fn nonzero_optional_padding_is_present_then_proves_real_nested_and_index_paths() {
    let kind = field("maybe_kind");
    let mut aligned_padding = optional_root_bytes();
    aligned_padding[kind.offset() + kind.size() - 1] = 0x44;
    let error = OptionalRoot::access(&aligned_padding)
        .expect_err("nonzero field-local alignment padding makes an all-zero enum present");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[ErrorPathSegment::Field("maybe_kind")],
    );
    assert_eq!(error_path_string(&error), "OptionalRoot.maybe_kind");

    let child = field("maybe_child");
    let mut child_padding = optional_root_bytes();
    child_padding[child.offset() + 1] = 0xc1;
    let error = OptionalRoot::access(&child_padding)
        .expect_err("nonzero inner child padding makes an all-zero child present");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[
            ErrorPathSegment::Field("maybe_child"),
            ErrorPathSegment::Field("required"),
        ],
    );
    assert_eq!(
        error_path_string(&error),
        "OptionalRoot.maybe_child.required"
    );

    let array = field("maybe_array");
    let mut first_index = optional_root_bytes();
    first_index[array.offset() + 1] = Required::One as u8;
    let error = OptionalRoot::access(&first_index)
        .expect_err("a later nonzero array element still proves the first element first");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[
            ErrorPathSegment::Field("maybe_array"),
            ErrorPathSegment::Index(0),
        ],
    );

    let mut second_index = optional_root_bytes();
    second_index[array.offset()] = Required::One as u8;
    let error = OptionalRoot::access(&second_index)
        .expect_err("array proof reaches the next invalid element in increasing order");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[
            ErrorPathSegment::Field("maybe_array"),
            ErrorPathSegment::Index(1),
        ],
    );
}

#[test]
fn every_byte_of_an_optional_storage_span_is_presence_significant() {
    #[repr(align(8))]
    struct AlignedStorage([u8; OptionalRoot::SCHEMA_SIZE]);

    let kind = field("maybe_kind");
    let baseline = AlignedStorage(optional_root_bytes());
    let mut initialized = AlignedStorage(baseline.0);
    OptionalRoot::access_mut(&mut initialized.0)
        .expect("all-zero optional is absent")
        .maybe_kind_mut()
        .set(Some(Required::One))
        .expect("valid enum initializes its inner wire value");
    let span = kind.offset()..kind.offset() + kind.size();
    let value_bytes: Vec<_> = span
        .clone()
        .filter(|index| initialized.0[*index] != baseline.0[*index])
        .collect();
    assert_eq!(
        value_bytes.len(),
        1,
        "the scalar inner wire has no Option presence byte"
    );
    for index in span {
        let mut candidate = AlignedStorage(baseline.0);
        if initialized.0[index] != baseline.0[index] {
            candidate.0[index] = initialized.0[index];
            assert_eq!(
                OptionalRoot::access(&candidate.0)
                    .expect("the initialized scalar byte is a valid Some")
                    .maybe_kind(),
                Some(Required::One)
            );
        } else {
            candidate.0[index] = 0xa5;
            let error = OptionalRoot::access(&candidate.0)
                .expect_err("every nonzero storage padding byte makes the optional present");
            assert_path(
                &error,
                ErrorKind::UnknownEnumValue,
                &[ErrorPathSegment::Field("maybe_kind")],
            );
        }
    }
}

#[test]
fn valid_child_padding_and_parent_padding_are_ignored_after_presence_proof() {
    let mut bytes = optional_root_bytes();
    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("absent optionals are valid");
        root.maybe_child_mut()
            .set(Some(Child {
                required: Required::One,
                payload: 23,
            }))
            .expect("valid child initializes");
    }
    let child = field("maybe_child");
    bytes[child.offset() + 1] = 0xc1;
    for index in root_padding_indices() {
        bytes[index] = 0xa5;
    }

    let view = OptionalRoot::access(&bytes)
        .expect("inner child padding and parent padding do not participate in child proof");
    assert_eq!(
        view.maybe_child()
            .map(|child| (child.required(), child.payload())),
        Some((Required::One, 23))
    );
    assert!(view.maybe_kind().is_none());
}

#[test]
fn option_mut_uses_short_live_reborrows_and_clears_only_its_complete_span() {
    let mut bytes = optional_root_bytes();
    let child_span = field_span("maybe_child");
    let parent_padding: Vec<_> = root_padding_indices().collect();
    for index in &parent_padding {
        bytes[*index] = 0x7b;
    }
    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("absent optionals are valid");
        let mut option = root.maybe_child_mut();
        assert!(option.get().is_none());
        assert!(option.get_mut().is_none());

        option
            .set(Some(Child {
                required: Required::One,
                payload: 23,
            }))
            .expect("absent to Some initializes after preflight");
        assert_eq!(
            option
                .get()
                .map(|child| (child.required(), child.payload())),
            Some((Required::One, 23))
        );
        {
            let mut child = option
                .get_mut()
                .expect("present child has a short mutable reborrow");
            child
                .payload_mut()
                .set(41)
                .expect("nested mutation through the live short borrow");
        }
        assert_eq!(
            option
                .get()
                .map(|child| (child.required(), child.payload())),
            Some((Required::One, 41)),
            "get rescans current storage rather than a cached logical value"
        );
        option
            .set(None)
            .expect("present to None clears the live option");
        assert!(
            option.get().is_none(),
            "get rescans the cleared storage rather than caching Some"
        );
        option
            .set(Some(Child {
                required: Required::One,
                payload: 41,
            }))
            .expect("cleared option can be initialized again");
    }

    bytes[child_span.start + 1] = 0xc1;
    let before_clear = bytes;
    {
        let mut root = OptionalRoot::access_mut(&mut bytes)
            .expect("present child with ignored padding is valid");
        root.maybe_child_mut()
            .set(None)
            .expect("Some to None clears the field");
    }
    assert!(bytes[child_span.clone()].iter().all(|byte| *byte == 0));
    for index in 0..bytes.len() {
        if !child_span.contains(&index) {
            assert_eq!(
                bytes[index], before_clear[index],
                "clear changed byte {index} outside its descriptor span"
            );
        }
    }
    assert!(parent_padding.iter().all(|index| bytes[*index] == 0x7b));
}

#[test]
fn option_mut_source_preflight_is_byte_exact_on_failure() {
    let mut bytes = optional_root_bytes();
    let before = bytes;
    let error = OptionalRoot::access_mut(&mut bytes)
        .expect("absent optionals are valid")
        .maybe_text_mut()
        .set(Some(EligibleTextChild {
            required: Required::One,
            text: "too-long",
        }))
        .expect_err("source exceeding child string capacity must fail before writes");
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(
        bytes, before,
        "failed OptionMut source preflight changed root bytes"
    );
}

#[test]
fn optional_patch_tri_state_promotion_and_from_logical_are_atomic() {
    let mut bytes = optional_root_bytes();
    let child_span = field_span("maybe_child");

    let before_retain = bytes;
    OptionalRoot::access_mut(&mut bytes)
        .expect("absent optionals are valid")
        .copy_from(&OptionalRootPatch::default())
        .expect("outer None patch retains every byte");
    assert_eq!(
        bytes, before_retain,
        "outer None must be a byte-exact no-op"
    );

    let incomplete = OptionalRootPatch {
        maybe_text: Some(Some(EligibleTextChildPatch {
            required: None,
            text: Some("too-long"),
        })),
        ..Default::default()
    };
    let before_incomplete = bytes;
    let error = OptionalRoot::access_mut(&mut bytes)
        .expect("absent optionals are valid")
        .copy_from(&incomplete)
        .expect_err("absent partial child must fail before its invalid source is inspected");
    assert_path(
        &error,
        ErrorKind::IncompleteOptionalInitialization,
        &[ErrorPathSegment::Field("maybe_text")],
    );
    assert_eq!(
        bytes, before_incomplete,
        "failed absent promotion changed root bytes"
    );

    let complete = OptionalRootPatch {
        maybe_child: Some(Some(ChildPatch {
            required: Some(Required::One.into()),
            payload: Some(23),
        })),
        maybe_array: Some(Some([Required::One, Required::Two])),
        ..Default::default()
    };
    OptionalRoot::access_mut(&mut bytes)
        .expect("absent optionals are valid")
        .copy_from(&complete)
        .expect("complete child and array patches promote absent optionals");
    let promoted = OptionalRoot::access(&bytes)
        .expect("promotion leaves valid wire bytes")
        .copy_into();
    assert_eq!(
        promoted.maybe_child,
        Some(Child {
            required: Required::One,
            payload: 23,
        })
    );
    assert_eq!(promoted.maybe_array, Some([Required::One, Required::Two]));

    let partial_present = OptionalRootPatch {
        maybe_child: Some(Some(ChildPatch {
            required: Some(Required::Two.into()),
            payload: None,
        })),
        ..Default::default()
    };
    OptionalRoot::access_mut(&mut bytes)
        .expect("present child is valid")
        .copy_from(&partial_present)
        .expect("partial child patch updates an already present optional");
    let present = OptionalRoot::access(&bytes)
        .expect("partial update leaves valid bytes")
        .maybe_child()
        .expect("child remains present");
    assert_eq!((present.required(), present.payload()), (Required::Two, 23));

    let clear = OptionalRootPatch {
        maybe_child: Some(None),
        ..Default::default()
    };
    OptionalRoot::access_mut(&mut bytes)
        .expect("present child is valid")
        .copy_from(&clear)
        .expect("Some(None) clears a present optional");
    assert!(bytes[child_span].iter().all(|byte| *byte == 0));

    let logical = OptionalRoot {
        before: 7,
        maybe_kind: None,
        maybe_child: None,
        maybe_array: None,
        maybe_text: None,
        maybe_tagged: None,
        after: 9,
    };
    let from_logical = OptionalRootPatch::from(logical);
    assert!(matches!(&from_logical.maybe_kind, Some(None)));
    assert!(matches!(&from_logical.maybe_child, Some(None)));
    assert!(matches!(&from_logical.maybe_array, Some(None)));
    assert!(matches!(&from_logical.maybe_text, Some(None)));
    assert!(matches!(&from_logical.maybe_tagged, Some(None)));
    OptionalRoot::access_mut(&mut bytes)
        .expect("cleared optionals are valid")
        .copy_from(&from_logical)
        .expect("a patch from a full logical value is complete");
    let copied = OptionalRoot::access(&bytes)
        .expect("From logical leaves a valid wire")
        .copy_into();
    assert_eq!(copied.before, 7);
    assert_eq!(copied.after, 9);
    assert!(copied.maybe_kind.is_none());
    assert!(copied.maybe_child.is_none());
    assert!(copied.maybe_array.is_none());
    assert!(copied.maybe_text.is_none());
    assert!(copied.maybe_tagged.is_none());
}

#[test]
fn optional_record_with_external_tag_initializes_payload_before_tag() {
    let mut bytes = optional_root_bytes();
    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("absent optionals are valid");
        root.maybe_tagged_mut()
            .set(Some(EligibleTaggedRecord {
                required: Required::One,
                tag: Required::Two,
                payload: Tagged::Two(TaggedPayload {
                    required: Required::One,
                }),
            }))
            .expect("absent tagged-containing record initializes to a valid final state");
    }
    let tagged = OptionalRoot::access(&bytes)
        .expect("payload-before-tag initialization leaves the root valid")
        .maybe_tagged()
        .expect("tagged record is present");
    assert_eq!(tagged.tag(), Required::Two);
    assert_eq!(
        tagged
            .payload()
            .two()
            .expect("selected external payload")
            .required(),
        Required::One
    );
}

#[test]
fn padded_optional_array_scans_elements_in_order_and_clears_the_complete_stride_span() {
    let mut bytes = padded_array_bytes();
    let children = padded_array_field();
    let FieldKind::Array(array) = children.kind() else {
        panic!("optional array must expose its inner array descriptor");
    };
    assert_eq!(
        (array.length(), array.stride()),
        (2, PaddedEligibleChild::SCHEMA_SIZE)
    );
    let required = PaddedEligibleChild::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "required")
        .expect("declared child required field");
    let payload = PaddedEligibleChild::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "payload")
        .expect("declared child payload field");
    assert!(
        required.offset() + 1 < payload.offset(),
        "the child has an internal padding byte after its required enum"
    );
    assert!(
        PaddedOptionalArrayRoot::access(&bytes)
            .expect("all-zero complete array storage is absent")
            .children()
            .is_none()
    );

    {
        let mut root = PaddedOptionalArrayRoot::access_mut(&mut bytes)
            .expect("absent optional array is valid");
        root.children_mut()
            .set(Some([
                PaddedEligibleChild {
                    required: Required::One,
                    payload: 23,
                },
                PaddedEligibleChild {
                    required: Required::Two,
                    payload: 41,
                },
            ]))
            .expect("valid padded children initialize the optional array");
    }
    assert_eq!(
        PaddedOptionalArrayRoot::access(&bytes)
            .expect("valid optional array materializes")
            .copy_into()
            .children,
        Some([
            PaddedEligibleChild {
                required: Required::One,
                payload: 23,
            },
            PaddedEligibleChild {
                required: Required::Two,
                payload: 41,
            },
        ])
    );

    let mut invalid_later = bytes;
    invalid_later[children.offset() + array.stride() + required.offset()] = 0;
    let error = PaddedOptionalArrayRoot::access(&invalid_later)
        .expect_err("the later present element must prove its invalid required field");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[
            ErrorPathSegment::Field("children"),
            ErrorPathSegment::Index(1),
            ErrorPathSegment::Field("required"),
        ],
    );
    assert_eq!(
        error_path_string(&error),
        "PaddedOptionalArrayRoot.children[1].required"
    );

    bytes[children.offset() + required.offset() + 1] = 0xc1;
    let before_clear = bytes;
    PaddedOptionalArrayRoot::access_mut(&mut bytes)
        .expect("nonzero child padding is ignored during optional array proof")
        .children_mut()
        .set(None)
        .expect("clear padded optional array");
    let span = children.offset()..children.offset() + children.size();
    assert!(bytes[span.clone()].iter().all(|byte| *byte == 0));
    for index in 0..bytes.len() {
        if !span.contains(&index) {
            assert_eq!(
                bytes[index], before_clear[index],
                "array clear changed byte {index} outside its descriptor span"
            );
        }
    }
}

#[test]
fn optional_text_storage_scans_ignored_capacity_before_proving_the_required_child() {
    let optional = field("maybe_text");
    let FieldKind::Schema { layout } = optional.kind() else {
        panic!("optional text child must expose its inner schema descriptor");
    };
    let text = layout
        .fields()
        .iter()
        .find(|field| field.name() == "text")
        .expect("declared child string metadata");
    let FieldKind::String(string) = text.kind() else {
        panic!("child text must expose string metadata");
    };
    assert_eq!(text.size(), string.data_offset() + string.capacity());
    let unused_capacity_byte =
        optional.offset() + text.offset() + string.data_offset() + string.capacity() - 1;
    #[repr(align(8))]
    struct Storage([u8; OptionalRoot::SCHEMA_SIZE]);

    let mut bytes = Storage(optional_root_bytes());
    assert_eq!(bytes.0[unused_capacity_byte], 0);
    bytes.0[unused_capacity_byte] = 0xa5;

    let error = OptionalRoot::access(&bytes.0)
        .expect_err("a nonzero unused string byte makes the complete optional span present");
    assert_path(
        &error,
        ErrorKind::UnknownEnumValue,
        &[
            ErrorPathSegment::Field("maybe_text"),
            ErrorPathSegment::Field("required"),
        ],
    );
    assert_eq!(
        error_path_string(&error),
        "OptionalRoot.maybe_text.required"
    );
}

#[test]
fn optional_borrowed_child_initializes_from_nonstatic_sources_without_retaining_them() {
    #[repr(align(8))]
    struct Storage([u8; OptionalRoot::SCHEMA_SIZE]);

    let mut direct = Storage(optional_root_bytes());
    {
        let source = String::from("one");
        OptionalRoot::access_mut(&mut direct.0)
            .expect("absent optionals are valid")
            .maybe_text_mut()
            .set(Some(EligibleTextChild {
                required: Required::One,
                text: source.as_str(),
            }))
            .expect("local source initializes optional child before it is dropped");
    }
    let direct_copy = OptionalRoot::access(&direct.0)
        .expect("wire storage owns the copied local source bytes")
        .copy_into();
    assert_eq!(
        direct_copy
            .maybe_text
            .expect("direct child is present")
            .text,
        "one"
    );

    let mut patched = Storage(optional_root_bytes());
    {
        let source = String::from("two");
        let patch = OptionalRootPatch {
            maybe_text: Some(Some(EligibleTextChildPatch {
                required: Some(Required::Two.into()),
                text: Some(source.as_str()),
            })),
            ..Default::default()
        };
        OptionalRoot::access_mut(&mut patched.0)
            .expect("absent optionals are valid")
            .copy_from(&patch)
            .expect("complete patch promotes from a local source before it is dropped");
    }
    let patched_copy = OptionalRoot::access(&patched.0)
        .expect("patched wire storage owns the copied local source bytes")
        .copy_into();
    let text = patched_copy.maybe_text.expect("patched child is present");
    assert_eq!((text.required, text.text), (Required::Two, "two"));
}
