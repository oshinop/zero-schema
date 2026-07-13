use zero_schema::{
    Endian, ErrorKind, ErrorPathSegment, FieldKind, PrimitiveKind, SchemaError, TypeKind,
    ValidationContext, ValidationFailure, ValidationOperation, ZeroSchema,
};

const FIELD_REJECTION: u32 = 713;
const WHOLE_REJECTION: u32 = 900;

fn validate_quota(quota: &u8, context: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    assert_eq!(context.layout().name(), "Policy");
    assert_eq!(context.field(), Some("quota"));
    assert_eq!(context.variant(), None);
    assert!(matches!(
        context.operation(),
        ValidationOperation::Decode | ValidationOperation::Encode
    ));

    if *quota == 13 {
        Err(ValidationFailure::new(
            FIELD_REJECTION,
            "quota 13 is reserved",
        ))
    } else {
        Ok(())
    }
}

fn validate_policy(
    policy: &Policy,
    context: &ValidationContext<'_>,
) -> zero_schema::ValidationResult {
    assert_eq!(context.layout().name(), "Policy");
    assert_eq!(context.field(), None);
    assert_eq!(context.variant(), None);
    assert!(matches!(
        context.operation(),
        ValidationOperation::Decode | ValidationOperation::Encode
    ));

    if policy.quota > policy.limit {
        Err(ValidationFailure::new(
            WHOLE_REJECTION,
            "quota exceeds limit",
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, ZeroSchema)]
#[zero(validate_with = validate_policy)]
struct Policy {
    #[zero(must_equal = 1)]
    version: u8,
    #[zero(range = 1..=100, validate_with = validate_quota)]
    quota: u8,
    limit: u8,
}

fn assert_leaf_error(
    error: &dyn SchemaError,
    kind: ErrorKind,
    segment: Option<ErrorPathSegment>,
    code: Option<u32>,
) {
    // These queries borrow static diagnostic data and require no allocation.
    assert_eq!(error.kind(), kind);
    assert_eq!(error.schema(), "Policy");
    assert_eq!(error.segment(), segment);
    assert_eq!(error.validation_code(), code);
    assert!(error.child().is_none());
}

fn main() {
    let layout = Policy::LAYOUT;
    assert_eq!(layout.name(), "Policy");
    assert_eq!(layout.kind(), TypeKind::Struct);
    assert_eq!(layout.size(), Policy::WIRE_SIZE);
    assert_eq!(layout.align(), Policy::WIRE_ALIGN);
    assert_eq!(layout.stride(), Policy::WIRE_STRIDE);
    assert_eq!(layout.fields().len(), 3);

    let quota_field = &layout.fields()[1];
    assert_eq!(quota_field.name(), "quota");
    assert_eq!(quota_field.declaration_index(), 1);
    assert!(quota_field.offset() + quota_field.size() <= layout.size());
    assert!(matches!(
        quota_field.kind(),
        FieldKind::Primitive {
            primitive: PrimitiveKind::U8,
            endian: Endian::Native,
        }
    ));

    let valid = Policy {
        version: 1,
        quota: 10,
        limit: 20,
    };
    let mut encoded = zero_schema::make_buffer_for!(Policy);
    valid.encode_into(encoded.as_bytes_mut()).unwrap();
    assert_eq!(Policy::parse(encoded.as_bytes()).unwrap(), valid);

    let mut rejected_output = zero_schema::make_buffer_for!(Policy);
    rejected_output.as_bytes_mut().fill(0xa5);
    let before = rejected_output.as_bytes().to_vec();
    let encode_error = Policy {
        version: 1,
        quota: 50,
        limit: 40,
    }
    .encode_into(rejected_output.as_bytes_mut())
    .unwrap_err();
    assert_leaf_error(
        &encode_error,
        ErrorKind::CustomValidation,
        None,
        Some(WHOLE_REJECTION),
    );
    assert_eq!(rejected_output.as_bytes(), before);
    // Regardless of whether bytes happen to be unchanged, output from an errored encode must not be published.

    let range_error = Policy {
        version: 1,
        quota: 101,
        limit: 101,
    }
    .encode_into(rejected_output.as_bytes_mut())
    .unwrap_err();
    assert_leaf_error(
        &range_error,
        ErrorKind::RangeViolation,
        Some(ErrorPathSegment::Field("quota")),
        None,
    );

    let equality_error = Policy {
        version: 2,
        quota: 10,
        limit: 20,
    }
    .encode_into(rejected_output.as_bytes_mut())
    .unwrap_err();
    assert_leaf_error(
        &equality_error,
        ErrorKind::MustEqualViolation,
        Some(ErrorPathSegment::Field("version")),
        None,
    );

    let mut malformed = encoded;
    malformed.as_bytes_mut()[quota_field.offset()] = 13;
    let decode_error = Policy::parse(malformed.as_bytes()).unwrap_err();
    assert_leaf_error(
        &decode_error,
        ErrorKind::CustomValidation,
        Some(ErrorPathSegment::Field("quota")),
        Some(FIELD_REJECTION),
    );

    #[cfg(feature = "alloc")]
    assert_eq!(
        zero_schema::error_path_string(&decode_error),
        "Policy.quota"
    );

    println!("validated Policy metadata and structured encode/decode errors");
}
