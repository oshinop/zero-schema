use core::fmt;

/// An error in the size, alignment, or bounds of a wire buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LayoutError {
    IncorrectSize { expected: usize, actual: usize },
    InsufficientBytes { required: usize, actual: usize },
    Misaligned { required: usize, address: usize },
    OffsetOverflow,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::IncorrectSize { expected, actual } => {
                write!(f, "incorrect size: expected {expected} bytes, got {actual}")
            }
            Self::InsufficientBytes { required, actual } => {
                write!(f, "insufficient bytes: required {required}, got {actual}")
            }
            Self::Misaligned { required, address } => {
                write!(
                    f,
                    "misaligned address 0x{address:x}: required alignment {required}"
                )
            }
            Self::OffsetOverflow => f.write_str("byte-range offset overflow"),
        }
    }
}

impl core::error::Error for LayoutError {}

/// A stable, allocation-free classification of a schema error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    Layout,
    InvalidBool,
    UnknownEnumValue,
    LengthOutOfBounds,
    LengthUnrepresentable,
    InvalidUtf8,
    MissingNul,
    UnknownUnionTag,
    CapacityExceeded,
    ArrayIndexOutOfBounds,
    ArrayLengthMismatch,
    TagMismatch,
    TagOnlyPatch,
    IncompleteUnionSwitch,
    IncompleteOptionalInitialization,
}

/// One component of a structured schema error path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorPathSegment {
    Field(&'static str),
    Index(usize),
    Variant(&'static str),
}

/// Common inspection interface implemented by all generated schema errors.
pub trait SchemaError: core::error::Error + 'static {
    fn kind(&self) -> ErrorKind;
    fn schema(&self) -> &'static str;
    fn segment(&self) -> Option<ErrorPathSegment>;
    fn child(&self) -> Option<&dyn SchemaError>;

    #[doc(hidden)]
    fn __fmt_leaf(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// Formats a schema error without allocation or recursion.
#[doc(hidden)]
pub fn __fmt_schema_error(error: &dyn SchemaError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(error.schema())?;

    let mut current = error;
    loop {
        if let Some(segment) = current.segment() {
            match segment {
                ErrorPathSegment::Field(name) | ErrorPathSegment::Variant(name) => {
                    f.write_str(".")?;
                    f.write_str(name)?;
                }
                ErrorPathSegment::Index(index) => write!(f, "[{index}]")?,
            }
        }

        match current.child() {
            Some(child) => current = child,
            None => break,
        }
    }

    f.write_str(": ")?;
    current.__fmt_leaf(f)
}

/// Returns only the schema and structured path portion of an error.
#[cfg(feature = "alloc")]
pub fn error_path_string(error: &dyn SchemaError) -> alloc::string::String {
    use alloc::string::String;

    let mut path = String::from(error.schema());
    let mut current = error;
    loop {
        if let Some(segment) = current.segment() {
            match segment {
                ErrorPathSegment::Field(name) | ErrorPathSegment::Variant(name) => {
                    path.push('.');
                    path.push_str(name);
                }
                ErrorPathSegment::Index(index) => {
                    use core::fmt::Write as _;

                    write!(&mut path, "[{index}]").expect("formatting into String is infallible");
                }
            }
        }
        match current.child() {
            Some(child) => current = child,
            None => return path,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::format;

    #[derive(Debug)]
    struct TestError {
        schema: &'static str,
        segment: Option<ErrorPathSegment>,
        child: Option<&'static TestError>,
        leaf: &'static str,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            __fmt_schema_error(self, f)
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::InvalidBool
        }

        fn schema(&self) -> &'static str {
            self.schema
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            self.segment
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            self.child.map(|child| child as &dyn SchemaError)
        }

        fn __fmt_leaf(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.leaf)
        }
    }

    static INDEX_LEAF: TestError = TestError {
        schema: "Message",
        segment: Some(ErrorPathSegment::Index(3)),
        child: None,
        leaf: "invalid sample",
    };
    static SAMPLES_ROOT: TestError = TestError {
        schema: "Message",
        segment: Some(ErrorPathSegment::Field("samples")),
        child: Some(&INDEX_LEAF),
        leaf: "unused root leaf",
    };
    static PATH_LEAF: TestError = TestError {
        schema: "Path",
        segment: Some(ErrorPathSegment::Field("path")),
        child: None,
        leaf: "deep failure",
    };
    static FILE_CHILD: TestError = TestError {
        schema: "Config",
        segment: Some(ErrorPathSegment::Variant("File")),
        child: Some(&PATH_LEAF),
        leaf: "unused child leaf",
    };
    static ROOT: TestError = TestError {
        schema: "Message",
        segment: Some(ErrorPathSegment::Field("config")),
        child: Some(&FILE_CHILD),
        leaf: "unused root leaf",
    };

    #[test]
    fn layout_display_is_stable() {
        assert_eq!(
            format!(
                "{}",
                LayoutError::IncorrectSize {
                    expected: 8,
                    actual: 3
                }
            ),
            "incorrect size: expected 8 bytes, got 3"
        );
        assert_eq!(
            format!(
                "{}",
                LayoutError::InsufficientBytes {
                    required: 9,
                    actual: 4
                }
            ),
            "insufficient bytes: required 9, got 4"
        );
        assert_eq!(
            format!(
                "{}",
                LayoutError::Misaligned {
                    required: 8,
                    address: 42
                }
            ),
            "misaligned address 0x2a: required alignment 8"
        );
        assert_eq!(
            format!("{}", LayoutError::OffsetOverflow),
            "byte-range offset overflow"
        );
    }

    #[test]
    fn schema_display_formats_index_and_traverses_child_errors() {
        assert_eq!(
            format!("{SAMPLES_ROOT}"),
            "Message.samples[3]: invalid sample"
        );
        assert_eq!(format!("{ROOT}"), "Message.config.File.path: deep failure");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn allocated_path_omits_leaf_and_matches_display_prefix() {
        let samples_path = error_path_string(&SAMPLES_ROOT);
        assert_eq!(samples_path, "Message.samples[3]");

        let path = error_path_string(&ROOT);
        assert_eq!(path, "Message.config.File.path");
        assert_eq!(format!("{ROOT}"), format!("{path}: deep failure"));
    }
}
