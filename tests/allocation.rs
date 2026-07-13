use core::fmt::{self, Write};
#[path = "support/counting_alloc.rs"]
mod counting_alloc;
use counting_alloc::{assert_instrumentation_works, zero_allocations};

use zero_schema::{SchemaError, ValidationContext, ValidationFailure, ZeroSchema};

fn reject(value: &u32, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    if *value == 99 {
        Err(ValidationFailure::new(99, "rejected"))
    } else {
        Ok(())
    }
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Direct {
    valid: bool,
    #[zero(validate_with = reject)]
    value: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Nested {
    prefix: u8,
    direct: Direct,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Tag {
    Empty = 1,
    Direct = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag)]
enum Payload {
    #[zero(tag = Tag::Empty)]
    Empty,
    #[zero(tag = Tag::Direct)]
    Direct(Direct),
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct External {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Payload,
}

struct StackText {
    bytes: [u8; 256],
    len: usize,
}
impl StackText {
    const fn new() -> Self {
        Self {
            bytes: [0; 256],
            len: 0,
        }
    }
}
impl Write for StackText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[test]
fn generated_paths_are_allocation_free() {
    // Prove the instrument is live before relying on zero counts. Setup and destruction are
    // deliberately outside every measured protocol operation.
    assert_instrumentation_works();

    let direct = Direct {
        valid: true,
        value: 7,
    };
    let nested = Nested {
        prefix: 3,
        direct: Direct {
            valid: true,
            value: 8,
        },
    };
    let payload = Payload::Direct(Direct {
        valid: true,
        value: 9,
    });
    let external = External {
        tag: Tag::Direct,
        payload: Payload::Direct(Direct {
            valid: true,
            value: 10,
        }),
    };
    let mut direct_bytes = zero_schema::make_buffer_for!(Direct);
    let mut nested_bytes = zero_schema::make_buffer_for!(Nested);
    let mut payload_bytes = zero_schema::make_buffer_for!(Payload);
    let mut external_bytes = zero_schema::make_buffer_for!(External);

    zero_allocations(|| direct.encode_into(direct_bytes.as_bytes_mut()).unwrap());
    zero_allocations(|| nested.encode_into(nested_bytes.as_bytes_mut()).unwrap());
    zero_allocations(|| payload.encode_into(payload_bytes.as_bytes_mut()).unwrap());
    zero_allocations(|| external.encode_into(external_bytes.as_bytes_mut()).unwrap());
    zero_allocations(|| assert_eq!(Direct::parse(direct_bytes.as_bytes()).unwrap(), direct));
    zero_allocations(|| assert_eq!(Nested::parse(nested_bytes.as_bytes()).unwrap(), nested));
    zero_allocations(|| assert_eq!(Payload::parse(payload_bytes.as_bytes()).unwrap(), payload));
    zero_allocations(|| {
        assert_eq!(
            External::parse(external_bytes.as_bytes()).unwrap(),
            external
        )
    });

    let encode_error = zero_allocations(|| {
        Direct {
            valid: true,
            value: 99,
        }
        .encode_into(direct_bytes.as_bytes_mut())
        .unwrap_err()
    });
    let mismatch_error = zero_allocations(|| {
        External {
            tag: Tag::Empty,
            payload: Payload::Direct(Direct {
                valid: true,
                value: 1,
            }),
        }
        .encode_into(external_bytes.as_bytes_mut())
        .unwrap_err()
    });

    let bool_offset = Nested::LAYOUT.fields()[1].offset() + Direct::LAYOUT.fields()[0].offset();
    nested_bytes.as_bytes_mut()[bool_offset] = 2;
    let nested_error = zero_allocations(|| Nested::parse(nested_bytes.as_bytes()).unwrap_err());

    let tag_offset = External::LAYOUT.fields()[0].offset();
    external_bytes.as_bytes_mut()[tag_offset] = 0xff;
    let tag_error = zero_allocations(|| External::parse(external_bytes.as_bytes()).unwrap_err());

    let mut output = StackText::new();
    zero_allocations(|| {
        for error in [
            &encode_error as &dyn SchemaError,
            &mismatch_error,
            &nested_error,
            &tag_error,
        ] {
            let mut cursor = Some(error);
            while let Some(node) = cursor {
                let _ = (
                    node.kind(),
                    node.schema(),
                    node.segment(),
                    node.validation_code(),
                );
                let source = core::error::Error::source(node);
                if let Some(child) = node.child() {
                    assert!(source.is_some());
                    cursor = Some(child);
                } else {
                    cursor = None;
                }
            }
            let mut source_cursor = core::error::Error::source(error);
            while let Some(source) = source_cursor {
                source_cursor = source.source();
            }
            write!(&mut output, "{error}").unwrap();
        }
    });
    assert!(output.len > 0);
}
