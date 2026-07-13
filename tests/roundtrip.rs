use core::ffi::CStr;
use widestring::{U16CStr, U16Str};
use zero_schema::ZeroSchema;

#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(endian = "native")]
struct Primitives {
    u8v: u8,
    i8v: i8,
    u16v: u16,
    i16v: i16,
    u32v: u32,
    i32v: i32,
    u64v: u64,
    i64v: i64,
    f32v: f32,
    f64v: f64,
    yes: bool,
    no: bool,
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Text<'a> {
    #[zero(capacity = 16)]
    utf8: &'a str,
    #[zero(capacity = 8)]
    c: &'a CStr,
    #[zero(capacity = 8)]
    wide: &'a U16Str,
    #[zero(capacity = 8)]
    wide_c: &'a U16CStr,
    fixed: &'a [u8; 4],
}

#[derive(Debug, PartialEq, ZeroSchema)]
struct Child {
    value: u32,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Nested {
    head: u8,
    child: Child,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct EmptyText<'a> {
    #[zero(capacity = 0, len_type = u8)]
    value: &'a str,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct Generic<T> {
    value: T,
}
#[derive(Debug, PartialEq, ZeroSchema)]
struct ConstBytes<'a, const N: usize> {
    value: &'a [u8; N],
}

#[derive(Debug, PartialEq, ZeroSchema)]
#[repr(u16)]
enum Code {
    A = 0x1234,
    B = 0xabcd,
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum Tag {
    Unit = 1,
    Data = 2,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = Tag)]
enum Choice {
    #[zero(tag = Tag::Unit)]
    Unit,
    #[zero(tag = Tag::Data)]
    Data(Child),
}
#[derive(ZeroSchema)]
#[repr(u8)]
enum UnitTag {
    A = 3,
    B = 4,
}
#[derive(Debug, PartialEq, ZeroSchema)]
#[zero(tag = UnitTag)]
enum AllUnit {
    #[zero(tag = UnitTag::A)]
    A,
    #[zero(tag = UnitTag::B)]
    B,
}
#[derive(ZeroSchema)]
struct ExternalChoice {
    tag: Tag,
    #[zero(tag_field = tag)]
    payload: Choice,
}

fn inside<T>(pointer: *const T, units: usize, bytes: &[u8]) {
    let start = bytes.as_ptr() as usize;
    let end = start + bytes.len();
    let value = pointer as usize;
    assert!(value >= start && value + units * core::mem::size_of::<T>() <= end);
}

#[test]
fn primitive_bits_bool_and_scalar_enum_roundtrip() {
    let values = [
        Primitives {
            u8v: u8::MAX,
            i8v: i8::MIN,
            u16v: u16::MAX,
            i16v: i16::MIN,
            u32v: u32::MAX,
            i32v: i32::MIN,
            u64v: u64::MAX,
            i64v: i64::MIN,
            f32v: f32::from_bits(0x7fc1_2345),
            f64v: f64::from_bits(0x7ff8_1234_5678_9abc),
            yes: true,
            no: false,
        },
        Primitives {
            u8v: 0,
            i8v: -1,
            u16v: 0,
            i16v: -1,
            u32v: 0,
            i32v: -1,
            u64v: 0,
            i64v: -1,
            f32v: -0.0,
            f64v: 0.0,
            yes: false,
            no: true,
        },
        Primitives {
            u8v: 1,
            i8v: 1,
            u16v: 1,
            i16v: 1,
            u32v: 1,
            i32v: 1,
            u64v: 1,
            i64v: 1,
            f32v: 0.0,
            f64v: -0.0,
            yes: true,
            no: false,
        },
    ];
    for value in values {
        let mut buffer = zero_schema::make_buffer_for!(Primitives);
        value.encode_into(buffer.as_bytes_mut()).unwrap();
        let got = Primitives::parse(buffer.as_bytes()).unwrap();
        assert_eq!(
            (
                got.u8v, got.i8v, got.u16v, got.i16v, got.u32v, got.i32v, got.u64v, got.i64v,
                got.yes, got.no
            ),
            (
                value.u8v, value.i8v, value.u16v, value.i16v, value.u32v, value.i32v, value.u64v,
                value.i64v, value.yes, value.no
            )
        );
        assert_eq!(got.f32v.to_bits(), value.f32v.to_bits());
        assert_eq!(got.f64v.to_bits(), value.f64v.to_bits());
    }
    let mut buffer = zero_schema::make_buffer_for!(Code);
    Code::B.encode_into(buffer.as_bytes_mut()).unwrap();
    assert_eq!(Code::parse(buffer.as_bytes()).unwrap(), Code::B);
}

#[test]
fn borrowed_and_permissive_string_roundtrip() {
    let c_bytes = [0xff, 0x80, 0];
    let c = CStr::from_bytes_with_nul(&c_bytes).unwrap();
    let wide_units = [0xd800, 0x0061];
    let wide = U16Str::from_slice(&wide_units);
    let wide_c_units = [0xdc00, 0x0062, 0];
    let wide_c = U16CStr::from_slice(&wide_c_units).unwrap();
    let original = Text {
        utf8: "a\0b",
        c,
        wide,
        wide_c,
        fixed: &[9, 8, 7, 6],
    };
    let buffer = original.encode().unwrap();
    let got = Text::parse(buffer.as_bytes()).unwrap();
    assert_eq!(got.utf8, original.utf8);
    assert_eq!(got.c.to_bytes_with_nul(), c_bytes);
    assert_eq!(got.wide.as_slice(), wide_units);
    assert_eq!(got.wide_c.as_slice_with_nul(), wide_c_units);
    assert_eq!(got.fixed, original.fixed);
    inside(got.utf8.as_ptr(), got.utf8.len(), buffer.as_bytes());
    inside(
        got.c.as_ptr(),
        got.c.to_bytes_with_nul().len(),
        buffer.as_bytes(),
    );
    inside(got.wide.as_ptr(), got.wide.len(), buffer.as_bytes());
    inside(
        got.wide_c.as_ptr(),
        got.wide_c.as_slice_with_nul().len(),
        buffer.as_bytes(),
    );
    inside(got.fixed.as_ptr(), got.fixed.len(), buffer.as_bytes());
}

#[test]
fn nested_generic_zero_capacity_and_unions_roundtrip() {
    let nested = Nested {
        head: 5,
        child: Child { value: 99 },
    };
    let mut n = zero_schema::make_buffer_for!(Nested);
    nested.encode_into(n.as_bytes_mut()).unwrap();
    assert_eq!(Nested::parse(n.as_bytes()).unwrap(), nested);
    let empty = EmptyText { value: "" };
    let mut e = zero_schema::make_buffer_for!(EmptyText<'static>);
    empty.encode_into(e.as_bytes_mut()).unwrap();
    let decoded = EmptyText::parse(e.as_bytes()).unwrap();
    assert_eq!(decoded.value, "");
    inside(decoded.value.as_ptr(), 0, e.as_bytes());
    let generic = Generic {
        value: Child { value: 7 },
    };
    let mut storage = [0; Generic::<Child>::WIRE_SIZE + Generic::<Child>::WIRE_ALIGN];
    let offset = storage.as_ptr().align_offset(Generic::<Child>::WIRE_ALIGN);
    let bytes = &mut storage[offset..offset + Generic::<Child>::WIRE_SIZE];
    generic.encode_into(bytes).unwrap();
    assert_eq!(Generic::<Child>::parse(bytes).unwrap(), generic);
    let fixed = ConstBytes::<3> { value: &[1, 2, 3] };
    let mut storage = [0; ConstBytes::<3>::WIRE_SIZE + ConstBytes::<3>::WIRE_ALIGN];
    let offset = storage.as_ptr().align_offset(ConstBytes::<3>::WIRE_ALIGN);
    let bytes = &mut storage[offset..offset + ConstBytes::<3>::WIRE_SIZE];
    fixed.encode_into(bytes).unwrap();
    let got = ConstBytes::<3>::parse(bytes).unwrap();
    assert_eq!(got.value, fixed.value);
    inside(got.value.as_ptr(), 3, bytes);
    for value in [Choice::Unit, Choice::Data(Child { value: 42 })] {
        let mut b = zero_schema::make_buffer_for!(Choice);
        value.encode_into(b.as_bytes_mut()).unwrap();
        assert_eq!(Choice::parse(b.as_bytes()).unwrap(), value);
    }
    let mut b = zero_schema::make_buffer_for!(AllUnit);
    AllUnit::B.encode_into(b.as_bytes_mut()).unwrap();
    assert_eq!(AllUnit::parse(b.as_bytes()).unwrap(), AllUnit::B);
    let external = ExternalChoice {
        tag: Tag::Data,
        payload: Choice::Data(Child { value: 55 }),
    };
    let mut b = zero_schema::make_buffer_for!(ExternalChoice);
    external.encode_into(b.as_bytes_mut()).unwrap();
    let got = ExternalChoice::parse(b.as_bytes()).unwrap();
    assert!(matches!(got.tag, Tag::Data));
    assert!(matches!(got.payload, Choice::Data(Child { value: 55 })));
}
