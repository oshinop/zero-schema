use core::fmt::{self, Write};

use zero_schema::{SchemaError, ZeroSchema};

#[path = "support/counting_alloc.rs"]
mod counting_alloc;
use counting_alloc::{assert_instrumentation_works, zero_allocations};

#[derive(Debug, PartialEq, ZeroSchema)]
struct Direct {
    valid: bool,
    value: u32,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Nested {
    marker: u8,
    direct: Direct,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u8)]
enum Kind {
    Empty = 1,
    Direct = 2,
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Kind)]
enum Internal {
    #[zero(tag = Kind::Empty)]
    Empty,
    #[zero(tag = Kind::Direct)]
    Direct(Direct),
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct External {
    kind: Kind,
    #[zero(tag_field = kind)]
    payload: Internal,
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
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[test]
#[ignore = "focused single-thread allocation measurement"]
fn generated_operations_and_errors_do_not_allocate() {
    assert_instrumentation_works();
    let direct = Direct {
        valid: true,
        value: 0x1122_3344,
    };
    let nested = Nested {
        marker: 7,
        direct: Direct {
            valid: true,
            value: 9,
        },
    };
    let internal = Internal::Direct(Direct {
        valid: true,
        value: 11,
    });
    let external = External {
        kind: Kind::Direct,
        payload: Internal::Direct(Direct {
            valid: true,
            value: 13,
        }),
    };

    let mut direct_buffer = zero_schema::make_buffer_for!(Direct);
    let mut nested_buffer = zero_schema::make_buffer_for!(Nested);
    let mut internal_buffer = zero_schema::make_buffer_for!(Internal);
    let mut external_buffer = zero_schema::make_buffer_for!(External);
    direct.encode_into(direct_buffer.as_bytes_mut()).unwrap();
    nested.encode_into(nested_buffer.as_bytes_mut()).unwrap();
    internal
        .encode_into(internal_buffer.as_bytes_mut())
        .unwrap();
    external
        .encode_into(external_buffer.as_bytes_mut())
        .unwrap();

    zero_allocations(|| {
        assert_eq!(Direct::parse(direct_buffer.as_bytes()).unwrap(), direct);
        assert_eq!(Nested::parse(nested_buffer.as_bytes()).unwrap(), nested);
        assert_eq!(
            Internal::parse(internal_buffer.as_bytes()).unwrap(),
            internal
        );
        assert_eq!(
            External::parse(external_buffer.as_bytes()).unwrap(),
            external
        );
    });

    zero_allocations(|| {
        direct.encode_into(direct_buffer.as_bytes_mut()).unwrap();
        nested.encode_into(nested_buffer.as_bytes_mut()).unwrap();
        internal
            .encode_into(internal_buffer.as_bytes_mut())
            .unwrap();
        external
            .encode_into(external_buffer.as_bytes_mut())
            .unwrap();
        let reparsed = Nested::parse(nested_buffer.as_bytes()).unwrap();
        assert_eq!(reparsed, nested);
    });

    let mut direct_invalid = zero_schema::make_buffer_for!(Direct);
    direct_invalid
        .as_bytes_mut()
        .copy_from_slice(direct_buffer.as_bytes());
    direct_invalid.as_bytes_mut()[0] = 2;
    let direct_error = zero_allocations(|| Direct::parse(direct_invalid.as_bytes()).unwrap_err());

    let mut nested_invalid = zero_schema::make_buffer_for!(Nested);
    nested_invalid
        .as_bytes_mut()
        .copy_from_slice(nested_buffer.as_bytes());
    let child_offset = Nested::LAYOUT.fields()[1].offset();
    nested_invalid.as_bytes_mut()[child_offset] = 2;
    let nested_error = zero_allocations(|| Nested::parse(nested_invalid.as_bytes()).unwrap_err());

    let mut direct_text = StackText::new();
    let mut nested_text = StackText::new();
    zero_allocations(|| {
        assert!(direct_error.child().is_none());
        let child = nested_error.child().unwrap();
        assert_eq!(child.schema(), "Direct");
        assert!(core::error::Error::source(&nested_error).is_some());
        write!(&mut direct_text, "{direct_error}").unwrap();
        write!(&mut nested_text, "{nested_error}").unwrap();
    });

    assert!(direct_text.len > 0);
    assert!(nested_text.len > direct_text.len);
}
