use crate::layout::LayoutDescriptor;

pub type ValidationResult = Result<(), ValidationFailure>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationFailure {
    code: u32,
    message: &'static str,
}

impl ValidationFailure {
    pub const fn new(code: u32, message: &'static str) -> Self {
        Self { code, message }
    }

    pub const fn code(&self) -> u32 {
        self.code
    }

    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl core::fmt::Display for ValidationFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (validation code {})", self.message, self.code)
    }
}

impl core::error::Error for ValidationFailure {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationOperation {
    Decode,
    Encode,
}

pub struct ValidationContext<'layout> {
    layout: &'layout LayoutDescriptor,
    field: Option<&'static str>,
    variant: Option<&'static str>,
    operation: ValidationOperation,
}

impl<'layout> ValidationContext<'layout> {
    pub const fn layout(&self) -> &'layout LayoutDescriptor {
        self.layout
    }

    pub const fn field(&self) -> Option<&'static str> {
        self.field
    }

    pub const fn variant(&self) -> Option<&'static str> {
        self.variant
    }

    pub const fn operation(&self) -> ValidationOperation {
        self.operation
    }

    #[doc(hidden)]
    pub const fn __field(
        layout: &'layout LayoutDescriptor,
        field: &'static str,
        operation: ValidationOperation,
    ) -> Self {
        Self {
            layout,
            field: Some(field),
            variant: None,
            operation,
        }
    }

    #[doc(hidden)]
    pub const fn __whole(
        layout: &'layout LayoutDescriptor,
        variant: Option<&'static str>,
        operation: ValidationOperation,
    ) -> Self {
        Self {
            layout,
            field: None,
            variant,
            operation,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::layout::{PaddingPolicy, TypeKind};
    use std::format;

    static LAYOUT: LayoutDescriptor = LayoutDescriptor::__new(
        "Record",
        TypeKind::Struct,
        4,
        4,
        4,
        PaddingPolicy::Ignore,
        &[],
        &[],
        &[],
        &[],
    );

    #[test]
    fn field_context_exposes_containing_layout_and_operation() {
        for operation in [ValidationOperation::Decode, ValidationOperation::Encode] {
            let context = ValidationContext::__field(&LAYOUT, "count", operation);
            assert!(core::ptr::eq(context.layout(), &LAYOUT));
            assert_eq!(context.field(), Some("count"));
            assert_eq!(context.variant(), None);
            assert_eq!(context.operation(), operation);
        }
    }

    #[test]
    fn whole_context_exposes_optional_selected_variant() {
        let decode = ValidationContext::__whole(&LAYOUT, None, ValidationOperation::Decode);
        assert!(core::ptr::eq(decode.layout(), &LAYOUT));
        assert_eq!(decode.field(), None);
        assert_eq!(decode.variant(), None);
        assert_eq!(decode.operation(), ValidationOperation::Decode);

        let encode =
            ValidationContext::__whole(&LAYOUT, Some("Selected"), ValidationOperation::Encode);
        assert!(core::ptr::eq(encode.layout(), &LAYOUT));
        assert_eq!(encode.field(), None);
        assert_eq!(encode.variant(), Some("Selected"));
        assert_eq!(encode.operation(), ValidationOperation::Encode);
    }

    #[test]
    fn validation_failure_exposes_stable_leaf() {
        let failure = ValidationFailure::new(17, "rejected");
        assert_eq!(failure.code(), 17);
        assert_eq!(failure.message(), "rejected");
        assert_eq!(format!("{failure}"), "rejected (validation code 17)");
    }
}
