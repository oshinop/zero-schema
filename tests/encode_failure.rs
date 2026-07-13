use std::cell::RefCell;
use std::ffi::CStr;
use zero_schema::{
    ErrorKind, SchemaError, ValidationContext, ValidationFailure, ValidationOperation, ZeroSchema,
};

thread_local! { static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; static TAG_FAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn event(name: &'static str) {
    EVENTS.with(|e| e.borrow_mut().push(name));
}
fn take() -> Vec<&'static str> {
    EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()))
}
fn unchanged<T, E: SchemaError>(
    value: &T,
    encode: impl FnOnce(&T, &mut [u8]) -> Result<(), E>,
    bytes: &mut [u8],
) -> ErrorKind {
    bytes.fill(0xa5);
    let before = bytes.to_vec();
    let kind = encode(value, bytes).unwrap_err().kind();
    assert_eq!(bytes, before);
    kind
}

fn field(value: &u8, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    assert_eq!(c.operation(), ValidationOperation::Encode);
    event("field");
    if *value == 3 {
        Err(ValidationFailure::new(7, "field"))
    } else {
        Ok(())
    }
}
fn later(_: &u8, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    event("later");
    Ok(())
}
fn whole(value: &Checks, c: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    assert_eq!(c.operation(), ValidationOperation::Encode);
    event("whole");
    if value.last == 9 {
        Err(ValidationFailure::new(9, "whole"))
    } else {
        Ok(())
    }
}
#[derive(ZeroSchema)]
#[zero(validate_with=whole)]
struct Checks {
    #[zero(range=0.0..=1.0)]
    ratio: f32,
    #[zero(range=1..=4, validate_with=field)]
    first: u8,
    #[zero(must_equal = 2)]
    equal: u8,
    #[zero(validate_with=later)]
    last: u8,
}
#[derive(ZeroSchema)]
struct Text<'a> {
    #[zero(capacity = 3)]
    text: &'a str,
    #[zero(capacity = 3)]
    c: &'a CStr,
}

fn payload(value: &u8, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    event("selected");
    if *value == 8 {
        Err(ValidationFailure::new(8, "selected"))
    } else {
        Ok(())
    }
}
#[derive(ZeroSchema)]
struct Payload {
    #[zero(validate_with=payload)]
    value: u8,
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum Tag {
    Unit = 1,
    Data = 2,
}
fn union(value: &Choice, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    event("union");
    if matches!(value, Choice::Unit) {
        Err(ValidationFailure::new(10, "union"))
    } else {
        Ok(())
    }
}
#[derive(ZeroSchema)]
#[zero(tag=Tag,validate_with=union)]
enum Choice {
    #[zero(tag=Tag::Unit)]
    Unit,
    #[zero(tag=Tag::Data)]
    Data(Payload),
}

fn external_tag(_: &Tag, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    event("tag");
    if TAG_FAIL.with(|v| v.get()) {
        Err(ValidationFailure::new(12, "tag"))
    } else {
        Ok(())
    }
}
fn external_parent(value: &Envelope, _: &ValidationContext<'_>) -> zero_schema::ValidationResult {
    event("parent");
    if value.reject_parent {
        Err(ValidationFailure::new(11, "parent"))
    } else {
        Ok(())
    }
}
#[derive(ZeroSchema)]
#[zero(validate_with=external_parent)]
struct Envelope {
    #[zero(tag_field=tag)]
    payload: Choice,
    #[zero(validate_with=external_tag)]
    tag: Tag,
    reject_parent: bool,
}

#[derive(ZeroSchema)]
struct NestedText<'a> {
    child: Text<'a>,
}

#[derive(Debug)]
struct WriteFailure;
impl core::fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("write failed")
    }
}
impl core::error::Error for WriteFailure {}
impl SchemaError for WriteFailure {
    fn kind(&self) -> ErrorKind {
        ErrorKind::CustomValidation
    }
    fn schema(&self) -> &'static str {
        "WriteChild"
    }
    fn segment(&self) -> Option<zero_schema::ErrorPathSegment> {
        None
    }
    fn child(&self) -> Option<&dyn SchemaError> {
        None
    }
    fn __fmt_leaf(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("write failed")
    }
}
static WRITE_LAYOUT: zero_schema::LayoutDescriptor = zero_schema::LayoutDescriptor::__new(
    "WriteChild",
    zero_schema::TypeKind::Struct,
    1,
    1,
    1,
    zero_schema::PaddingPolicy::Ignore,
    &[],
    &[],
    &[],
    &[],
);
struct WriteChild;
impl zero_schema::ZeroSchemaType for WriteChild {
    type Wire = zero_schema::__private::U8;
    type DecodeError = WriteFailure;
    type EncodeError = WriteFailure;
    const WIRE_SIZE: usize = 1;
    const WIRE_ALIGN: usize = 1;
    const WIRE_STRIDE: usize = 1;
    const LAYOUT: &'static zero_schema::LayoutDescriptor = &WRITE_LAYOUT;
}
impl<'a> zero_schema::__private::DecodeWire<'a> for WriteChild {
    fn decode_at(_: zero_schema::DecodeInput<'a, Self::Wire>) -> Result<Self, Self::DecodeError> {
        Ok(Self)
    }
}
impl zero_schema::__private::EncodeWire for WriteChild {
    fn validate_encode(&self) -> Result<(), Self::EncodeError> {
        Ok(())
    }
    fn encode_at(
        &self,
        d: &mut zero_schema::__private::Prezeroed<'_>,
    ) -> Result<(), Self::EncodeError> {
        d.write(0, &[0x3c]).unwrap();
        Err(WriteFailure)
    }
}
#[derive(ZeroSchema)]
struct WriteParent {
    before: u8,
    child: WriteChild,
    after: u8,
}

#[test]
fn post_write_error_invalidates_only_the_supplied_root() {
    let mut storage = [0xd7; WriteParent::WIRE_SIZE + 2];
    let root = &mut storage[1..=WriteParent::WIRE_SIZE];
    WriteParent {
        before: 1,
        child: WriteChild,
        after: 2,
    }
    .encode_into(root)
    .unwrap_err();
    assert_eq!(storage[0], 0xd7);
    assert_eq!(storage[WriteParent::WIRE_SIZE + 1], 0xd7);
    assert!(
        storage[1..=WriteParent::WIRE_SIZE]
            .iter()
            .any(|b| *b != 0xd7)
    );
}

#[test]
fn layout_and_all_supported_semantic_preflight_failures_preserve_sentinels() {
    let good = Checks {
        ratio: 0.5,
        first: 2,
        equal: 2,
        last: 1,
    };
    let mut b = zero_schema::make_buffer_for!(Checks);
    let mut short = vec![0xa5; Checks::WIRE_SIZE - 1];
    let before = short.clone();
    assert_eq!(
        good.encode_into(&mut short).unwrap_err().kind(),
        ErrorKind::Layout
    );
    assert_eq!(short, before);
    let mut storage = [0xa5; Checks::WIRE_SIZE + Checks::WIRE_ALIGN];
    let base = storage.as_ptr() as usize;
    let off = (0..Checks::WIRE_ALIGN)
        .find(|o| (base + o) % Checks::WIRE_ALIGN != 0)
        .unwrap();
    let dst = &mut storage[off..off + Checks::WIRE_SIZE];
    let before = dst.to_vec();
    assert_eq!(good.encode_into(dst).unwrap_err().kind(), ErrorKind::Layout);
    assert_eq!(dst, before);

    take();
    assert_eq!(
        unchanged(
            &Checks {
                ratio: f32::NAN,
                first: 2,
                equal: 2,
                last: 1
            },
            |v, d| v.encode_into(d),
            b.as_bytes_mut()
        ),
        ErrorKind::RangeViolation
    );
    assert!(take().is_empty());
    assert_eq!(
        unchanged(
            &Checks {
                ratio: 0.5,
                first: 5,
                equal: 2,
                last: 1
            },
            |v, d| v.encode_into(d),
            b.as_bytes_mut()
        ),
        ErrorKind::RangeViolation
    );
    assert!(take().is_empty());
    assert_eq!(
        unchanged(
            &Checks {
                ratio: 0.5,
                first: 2,
                equal: 3,
                last: 1
            },
            |v, d| v.encode_into(d),
            b.as_bytes_mut()
        ),
        ErrorKind::MustEqualViolation
    );
    assert_eq!(take(), ["field"]);
    assert_eq!(
        unchanged(
            &Checks {
                ratio: 0.5,
                first: 3,
                equal: 2,
                last: 1
            },
            |v, d| v.encode_into(d),
            b.as_bytes_mut()
        ),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["field"]);
    assert_eq!(
        unchanged(
            &Checks {
                ratio: 0.5,
                first: 2,
                equal: 2,
                last: 9
            },
            |v, d| v.encode_into(d),
            b.as_bytes_mut()
        ),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["field", "later", "whole"]);

    let c = c"abc";
    let mut tb = zero_schema::make_buffer_for!(Text);
    assert_eq!(
        unchanged(
            &Text { text: "four", c },
            |v, d| v.encode_into(d),
            tb.as_bytes_mut()
        ),
        ErrorKind::CapacityExceeded
    );
    let too_long = c"abcd";
    assert_eq!(
        unchanged(
            &Text {
                text: "ok",
                c: too_long
            },
            |v, d| v.encode_into(d),
            tb.as_bytes_mut()
        ),
        ErrorKind::CapacityExceeded
    );
    let boundary = c"ab";
    Text {
        text: "ok",
        c: boundary,
    }
    .encode_into(tb.as_bytes_mut())
    .unwrap();

    let mut nb = zero_schema::make_buffer_for!(NestedText);
    assert_eq!(
        unchanged(
            &NestedText {
                child: Text { text: "four", c }
            },
            |v, d| v.encode_into(d),
            nb.as_bytes_mut()
        ),
        ErrorKind::CapacityExceeded
    );

    let mut ub = zero_schema::make_buffer_for!(Choice);
    take();
    assert_eq!(
        unchanged(
            &Choice::Data(Payload { value: 8 }),
            |v, d| v.encode_into(d),
            ub.as_bytes_mut()
        ),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["selected"]);
    assert_eq!(
        unchanged(&Choice::Unit, |v, d| v.encode_into(d), ub.as_bytes_mut()),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["union"]);
    take();
    Choice::Data(Payload { value: 1 })
        .encode_into(ub.as_bytes_mut())
        .unwrap();
    assert_eq!(take(), ["selected", "union"]);
    let mut eb = zero_schema::make_buffer_for!(Envelope);
    take();
    let mismatch = Envelope {
        tag: Tag::Unit,
        payload: Choice::Data(Payload { value: 8 }),
        reject_parent: true,
    };
    assert_eq!(
        unchanged(&mismatch, |v, d| v.encode_into(d), eb.as_bytes_mut()),
        ErrorKind::TagMismatch
    );
    assert!(take().is_empty());
    let selected = Envelope {
        tag: Tag::Data,
        payload: Choice::Data(Payload { value: 8 }),
        reject_parent: false,
    };
    assert_eq!(
        unchanged(&selected, |v, d| v.encode_into(d), eb.as_bytes_mut()),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["selected"]);
    let parent = Envelope {
        tag: Tag::Data,
        payload: Choice::Data(Payload { value: 1 }),
        reject_parent: true,
    };
    assert_eq!(
        unchanged(&parent, |v, d| v.encode_into(d), eb.as_bytes_mut()),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["selected", "union", "tag", "parent"]);
    TAG_FAIL.with(|v| v.set(true));
    let tag_failure = Envelope {
        tag: Tag::Data,
        payload: Choice::Data(Payload { value: 1 }),
        reject_parent: false,
    };
    assert_eq!(
        unchanged(&tag_failure, |v, d| v.encode_into(d), eb.as_bytes_mut()),
        ErrorKind::CustomValidation
    );
    assert_eq!(take(), ["selected", "union", "tag"]);
    TAG_FAIL.with(|v| v.set(false));
    take();
    Envelope {
        tag: Tag::Data,
        payload: Choice::Data(Payload { value: 1 }),
        reject_parent: false,
    }
    .encode_into(eb.as_bytes_mut())
    .unwrap();
    assert_eq!(take(), ["selected", "union", "tag", "parent"]);
}
