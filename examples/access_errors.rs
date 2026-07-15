use core::{
    error::Error as _,
    mem::{align_of, size_of},
};

use zero_schema::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError, error_path_string, zero};

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Mode {
    Ready = 1,
}

#[zero(align = 4)]
struct Status {
    active: bool,
    mode: Mode,
}

// These are explicit producer-provided records, not Rust-initialized schemas.
const READY_MODE_PRODUCER: [u8; Mode::SCHEMA_SIZE] = [Mode::Ready as u8];
const UNKNOWN_MODE_PRODUCER: [u8; Mode::SCHEMA_SIZE] = [99];
const INVALID_BOOLEAN_PRODUCER: [u8; Status::SCHEMA_SIZE] = [2, Mode::Ready as u8, 0xa1, 0xa2];
const UNKNOWN_ENUM_PRODUCER: [u8; Status::SCHEMA_SIZE] = [1, 99, 0xb1, 0xb2];

const _: [(); 1] = [(); Mode::SCHEMA_SIZE];
const _: [(); 1] = [(); Mode::SCHEMA_ALIGN];
const _: [(); 4] = [(); Status::SCHEMA_SIZE];
const _: [(); 4] = [(); Status::SCHEMA_ALIGN];

#[repr(C, align(1))]
struct ProducerMode {
    bytes: [u8; Mode::SCHEMA_SIZE],
}

#[repr(C, align(4))]
struct ProducerStatus {
    bytes: [u8; Status::SCHEMA_SIZE],
}

#[repr(C, align(4))]
struct ShortStatusProducer {
    bytes: [u8; Status::SCHEMA_SIZE - 1],
}

#[repr(C, align(4))]
struct MisalignedStatusProducer {
    bytes: [u8; Status::SCHEMA_SIZE + 1],
}

fn assert_root_error<E: SchemaError>(
    error: &E,
    expected_schema: &'static str,
    expected_kind: ErrorKind,
) {
    assert_eq!(error.kind(), expected_kind);
    assert_eq!(error.schema(), expected_schema);
    assert_eq!(error.segment(), None);
    assert!(error.child().is_none());
    assert_eq!(error_path_string(error), expected_schema);
}

fn assert_field_error<E: SchemaError>(
    error: &E,
    expected_kind: ErrorKind,
    expected_field: &'static str,
    expected_path: &'static str,
    expected_leaf: bool,
) {
    assert_eq!(error.kind(), expected_kind);
    assert_eq!(
        error.segment(),
        Some(ErrorPathSegment::Field(expected_field))
    );
    match (error.child(), expected_leaf) {
        (None, false) => {}
        (Some(leaf), true) => {
            assert_eq!(leaf.segment(), None);
            assert!(leaf.child().is_none());
        }
        (None, true) => panic!("field error must retain its leaf cause"),
        (Some(_), false) => panic!("field error unexpectedly retained a leaf cause"),
    }
    assert_eq!(error_path_string(error), expected_path);
}

fn main() {
    let mut mode_producer = ProducerMode {
        bytes: READY_MODE_PRODUCER,
    };
    assert_eq!(
        (size_of::<ProducerMode>(), align_of::<ProducerMode>()),
        (Mode::SCHEMA_SIZE, Mode::SCHEMA_ALIGN)
    );

    let mode = Mode::access(&mode_producer.bytes).expect("reviewed scalar producer byte is valid");
    assert_eq!(mode.get(), Mode::Ready);
    assert_eq!(mode.copy_into(), Mode::Ready);

    {
        let mut mode = Mode::access_mut(&mut mode_producer.bytes)
            .expect("reviewed scalar producer byte remains valid for mutation");
        assert_eq!(mode.get(), Mode::Ready);
        mode.set(Mode::Ready)
            .expect("a declared scalar enum value is writable");
        mode.copy_from(&ModePatch::from(Mode::Ready))
            .expect("a declared scalar enum patch is writable");
        assert_eq!(mode.copy_into(), Mode::Ready);
    }
    assert_eq!(
        Mode::access(&mode_producer.bytes)
            .expect("successful scalar mutation remains valid")
            .get(),
        Mode::Ready
    );

    let unknown_mode = ProducerMode {
        bytes: UNKNOWN_MODE_PRODUCER,
    };
    let Err(error) = Mode::access(&unknown_mode.bytes) else {
        panic!("unknown scalar enum discriminant must not yield a capability");
    };
    assert_root_error(&error, "Mode", ErrorKind::UnknownEnumValue);
    println!("scalar root rejected unknown enum value: {error}");

    let invalid_boolean = ProducerStatus {
        bytes: INVALID_BOOLEAN_PRODUCER,
    };
    let Err(error) = Status::access(&invalid_boolean.bytes) else {
        panic!("invalid Boolean must not yield a capability");
    };
    assert_field_error(
        &error,
        ErrorKind::InvalidBool,
        "active",
        "Status.active",
        false,
    );
    println!("eager access rejected invalid Boolean: {error}");

    let unknown_enum = ProducerStatus {
        bytes: UNKNOWN_ENUM_PRODUCER,
    };
    let Err(error) = Status::access(&unknown_enum.bytes) else {
        panic!("unknown record enum discriminant must not yield a capability");
    };
    assert_field_error(
        &error,
        ErrorKind::UnknownEnumValue,
        "mode",
        "Status.mode",
        true,
    );
    println!("eager access rejected unknown enum value: {error}");

    let short = ShortStatusProducer {
        bytes: [1, Mode::Ready as u8, 0xa1],
    };
    let Err(error) = Status::access(&short.bytes) else {
        panic!("short root span must not yield a capability");
    };
    assert_root_error(&error, "Status", ErrorKind::Layout);
    assert!(matches!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::IncorrectSize { expected, actual })
            if *expected == Status::SCHEMA_SIZE && *actual == Status::SCHEMA_SIZE - 1
    ));

    let misaligned = MisalignedStatusProducer {
        bytes: [0, 1, Mode::Ready as u8, 0xa1, 0xa2],
    };
    assert_eq!(align_of::<MisalignedStatusProducer>(), Status::SCHEMA_ALIGN);
    assert_eq!(
        misaligned.bytes.as_ptr() as usize % Status::SCHEMA_ALIGN,
        0,
        "the producer wrapper supplies an aligned base"
    );
    let misaligned_bytes = &misaligned.bytes[1..];
    assert_eq!(misaligned_bytes.len(), Status::SCHEMA_SIZE);
    assert_ne!(misaligned_bytes.as_ptr() as usize % Status::SCHEMA_ALIGN, 0);
    let Err(error) = Status::access(misaligned_bytes) else {
        panic!("misaligned root span must not yield a capability");
    };
    assert_root_error(&error, "Status", ErrorKind::Layout);
    assert!(matches!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<LayoutError>()),
        Some(LayoutError::Misaligned { required, .. }) if *required == Status::SCHEMA_ALIGN
    ));
}
