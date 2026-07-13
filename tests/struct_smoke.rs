use core::ffi::CStr;
use core::mem::{align_of_val, size_of, size_of_val};

use widestring::U16CStr;
use zero_schema::{
    ErrorKind, ErrorPathSegment, SchemaError, ValidationContext, ValidationFailure,
    ValidationResult, ZeroSchema,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[zero(endian = "native")]
pub struct Header<'a> {
    pub version: u16,
    #[zero(capacity = 32, tail = "zero")]
    pub producer: &'a CStr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ZeroSchema)]
#[zero(endian = "native", validate_with = validate_file)]
pub struct FileConfig<'a> {
    pub flags: u32,
    #[zero(capacity = 260, tail = "zero")]
    pub path: &'a U16CStr,
}

fn validate_file(value: &FileConfig<'_>, _: &ValidationContext<'_>) -> ValidationResult {
    if value.path.is_empty() {
        return Err(ValidationFailure::new(2001, "file path must not be empty"));
    }
    Ok(())
}

fn field(
    name: &str,
    layout: &'static zero_schema::LayoutDescriptor,
) -> &'static zero_schema::FieldDescriptor {
    layout
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .unwrap()
}

fn assert_buffer_contract<B>(
    buffer: &B,
    bytes: &[u8],
    schema_size: usize,
    schema_align: usize,
    schema_stride: usize,
) {
    assert_eq!(bytes, vec![0; schema_size]);
    assert_eq!(size_of_val(buffer), schema_stride);
    assert_eq!(align_of_val(buffer), schema_align);
    assert_eq!(bytes.as_ptr().align_offset(schema_align), 0);
}

fn assert_in_buffer<T>(pointer: *const T, units: usize, bytes: &[u8]) {
    let start = bytes.as_ptr() as usize;
    let end = start + bytes.len();
    let value_start = pointer as usize;
    let value_end = value_start + units * size_of::<T>();
    assert!(value_start >= start && value_end <= end);
}

#[test]
fn generated_buffers_encode_parse_prefix_and_borrow_from_input() {
    let mut header = zero_schema::make_buffer_for!(Header);
    assert_buffer_contract(
        &header,
        header.as_bytes(),
        Header::WIRE_SIZE,
        Header::WIRE_ALIGN,
        Header::WIRE_STRIDE,
    );
    header.as_bytes_mut().fill(0xa5);
    let header_value = Header {
        version: 3,
        producer: c"worker-service",
    };
    header_value.encode_into(header.as_bytes_mut()).unwrap();
    assert_eq!(&header.as_bytes()[..2], &3u16.to_ne_bytes());
    let producer = field("producer", Header::LAYOUT);
    assert_eq!(
        &header.as_bytes()[producer.offset()..producer.offset() + 15],
        b"worker-service\0"
    );
    assert!(
        header.as_bytes()[producer.offset() + 15..producer.offset() + 32]
            .iter()
            .all(|byte| *byte == 0)
    );
    let parsed = Header::parse(header.as_bytes()).unwrap();
    assert_eq!(parsed, header_value);
    assert_in_buffer(
        parsed.producer.as_ptr(),
        parsed.producer.to_bytes_with_nul().len(),
        header.as_bytes(),
    );
    assert_eq!(header_value.encoded_len(), Header::WIRE_SIZE);
    let mut prefixed = header.as_bytes().to_vec();
    prefixed.extend_from_slice(&[9, 8, 7]);
    let (parsed_prefix, remainder) = Header::parse_prefix(&prefixed).unwrap();
    assert_eq!(parsed_prefix, header_value);
    assert_eq!(remainder, &[9, 8, 7]);

    let path_units = [b'C' as u16, b':' as u16, b'/' as u16, b'x' as u16, 0];
    let path = U16CStr::from_slice(&path_units).unwrap();
    let value = FileConfig {
        flags: 0x0102_0304,
        path,
    };
    let mut file = zero_schema::make_buffer_for!(FileConfig<'static>);
    assert_buffer_contract(
        &file,
        file.as_bytes(),
        FileConfig::WIRE_SIZE,
        FileConfig::WIRE_ALIGN,
        FileConfig::WIRE_STRIDE,
    );
    value.encode_into(file.as_bytes_mut()).unwrap();
    assert_eq!(&file.as_bytes()[..4], &value.flags.to_ne_bytes());
    let path_field = field("path", FileConfig::LAYOUT);
    for (index, unit) in path_units.iter().enumerate() {
        assert_eq!(
            &file.as_bytes()[path_field.offset() + index * 2..path_field.offset() + index * 2 + 2],
            &unit.to_ne_bytes()
        );
    }
    assert!(
        file.as_bytes()[path_field.offset() + path_units.len() * 2..path_field.offset() + 520]
            .iter()
            .all(|byte| *byte == 0)
    );
    let decoded = FileConfig::parse(file.as_bytes()).unwrap();
    assert_eq!(decoded, value);
    assert_in_buffer(
        decoded.path.as_ptr(),
        decoded.path.as_slice_with_nul().len(),
        file.as_bytes(),
    );
    assert_eq!(value.encoded_len(), FileConfig::WIRE_SIZE);
    let mut input = file.as_bytes().to_vec();
    input.extend_from_slice(&[1, 2]);
    let (decoded_prefix, remainder) = FileConfig::parse_prefix(&input).unwrap();
    assert_eq!(decoded_prefix, value);
    assert_eq!(remainder, &[1, 2]);
}

fn assert_decode_error(
    error: &FileConfigDecodeError,
    kind: ErrorKind,
    code: Option<u32>,
    display: &str,
) {
    assert_eq!(error.kind(), kind);
    assert_eq!(error.schema(), "FileConfig");
    assert_eq!(error.validation_code(), code);
    assert_eq!(error.to_string(), display);
}

#[test]
fn aligned_mutations_report_exact_string_and_validator_errors() {
    let units = [b'x' as u16, 0];
    let valid = FileConfig {
        flags: 7,
        path: U16CStr::from_slice(&units).unwrap(),
    };
    let path = field("path", FileConfig::LAYOUT);

    let mut missing = zero_schema::make_buffer_for!(FileConfig<'static>);
    valid.encode_into(missing.as_bytes_mut()).unwrap();
    for bytes in missing.as_bytes_mut()[path.offset()..path.offset() + 520].chunks_exact_mut(2) {
        bytes.copy_from_slice(&1u16.to_ne_bytes());
    }
    let error = FileConfig::parse(missing.as_bytes()).unwrap_err();
    assert_decode_error(
        &error,
        ErrorKind::MissingNul,
        None,
        "FileConfig.path: missing NUL terminator",
    );
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("path")));
    assert!(core::error::Error::source(&error).is_none());

    let mut tail = zero_schema::make_buffer_for!(FileConfig<'static>);
    valid.encode_into(tail.as_bytes_mut()).unwrap();
    tail.as_bytes_mut()[path.offset() + 4..path.offset() + 6].copy_from_slice(&9u16.to_ne_bytes());
    let error = FileConfig::parse(tail.as_bytes()).unwrap_err();
    assert_decode_error(
        &error,
        ErrorKind::NonZeroTail,
        None,
        "FileConfig.path: nonzero tail at logical offset 2",
    );
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("path")));
    assert!(core::error::Error::source(&error).is_none());

    let mut empty = zero_schema::make_buffer_for!(FileConfig<'static>);
    valid.encode_into(empty.as_bytes_mut()).unwrap();
    empty.as_bytes_mut()[path.offset()..path.offset() + 2].copy_from_slice(&0u16.to_ne_bytes());
    empty.as_bytes_mut()[path.offset() + 2..path.offset() + 4].copy_from_slice(&0u16.to_ne_bytes());
    let error = FileConfig::parse(empty.as_bytes()).unwrap_err();
    assert_decode_error(
        &error,
        ErrorKind::CustomValidation,
        Some(2001),
        "FileConfig: file path must not be empty (validation code 2001)",
    );
    assert_eq!(error.segment(), None);
    let source = core::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<ValidationFailure>()
        .unwrap();
    assert_eq!(
        (source.code(), source.message()),
        (2001, "file path must not be empty")
    );
}

#[test]
fn encode_failures_are_transactional() {
    let long_units = vec![1u16; 260];
    let mut terminated = long_units;
    terminated.push(0);
    let too_long = FileConfig {
        flags: 1,
        path: U16CStr::from_slice(&terminated).unwrap(),
    };
    let mut destination = zero_schema::make_buffer_for!(FileConfig<'static>);
    destination.as_bytes_mut().fill(0xa5);
    let before = destination.as_bytes().to_vec();
    let error = too_long
        .encode_into(destination.as_bytes_mut())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("path")));
    assert_eq!(
        error.to_string(),
        "FileConfig.path: length 261 exceeds encoding capacity 260"
    );
    assert_eq!(destination.as_bytes(), before);

    let empty_units = [0u16];
    let empty = FileConfig {
        flags: 1,
        path: U16CStr::from_slice(&empty_units).unwrap(),
    };
    let error = empty.encode_into(destination.as_bytes_mut()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CustomValidation);
    assert_eq!(error.validation_code(), Some(2001));
    assert_eq!(
        error.to_string(),
        "FileConfig: file path must not be empty (validation code 2001)"
    );
    assert_eq!(destination.as_bytes(), before);
}
