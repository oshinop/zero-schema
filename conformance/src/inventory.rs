use core::mem::{align_of, offset_of, size_of};

use widestring::{U16CStr, U16Str};
use zero_schema::{
    Endian, FieldKind, IntegerRepr, LayoutDescriptor, LengthRepr, ScalarEnum, StringEncoding,
    TaggedUnion, TailPolicy, TypeKind, ZeroSchemaType,
};
use zero_schema_schema_corpus::conformance::*;

use crate::report::HarnessError;

type Pairs = Vec<(u64, u64)>;

pub(crate) struct Case {
    pub(crate) case_id: u32,
    #[allow(dead_code)]
    pub(crate) root_id: &'static str,
    #[allow(dead_code)]
    pub(crate) expected_key_values: fn() -> Result<Pairs, HarnessError>,
    pub(crate) rust_bytes: fn() -> Result<Vec<u8>, HarnessError>,
    pub(crate) rust_observe: fn(&[u8]) -> Result<Pairs, HarnessError>,
}

fn u(what: &'static str, value: usize) -> Result<u64, HarnessError> {
    u64::try_from(value).map_err(|_| HarnessError::ValueOutOfRange { what, value })
}
fn bad(_: impl core::fmt::Display) -> HarnessError {
    HarnessError::InvalidData("schema codec rejected conformance data")
}
fn field<'a>(
    layout: &'a LayoutDescriptor,
    name: &str,
) -> Result<&'a zero_schema::FieldDescriptor, HarnessError> {
    layout
        .fields()
        .iter()
        .find(|f| f.name() == name)
        .ok_or(HarnessError::InvalidData("missing layout field"))
}
fn basic_layout<T: ZeroSchemaType>(id: u32) -> Result<Pairs, HarnessError> {
    let b = u64::from(id) * 1000;
    let mut out = vec![
        (b + 1, u("wire size", T::WIRE_SIZE)?),
        (b + 2, u("wire align", T::WIRE_ALIGN)?),
        (b + 3, u("wire stride", T::WIRE_STRIDE)?),
    ];
    for (i, f) in T::LAYOUT.fields().iter().enumerate() {
        let k = b + 10 + u64::try_from(i).unwrap() * 3;
        out.extend([
            (k, u("field offset", f.offset())?),
            (k + 1, u("field size", f.size())?),
            (k + 2, u("field align", f.align())?),
        ]);
    }
    Ok(out)
}
fn endian(e: Endian) -> u64 {
    match e {
        Endian::Native => 0,
        Endian::Little => 1,
        Endian::Big => 2,
        _ => unreachable!(),
    }
}
fn int_width(r: IntegerRepr) -> u64 {
    match r {
        IntegerRepr::U8 => 1,
        IntegerRepr::U16 => 2,
        IntegerRepr::U32 => 4,
        _ => unreachable!(),
    }
}
fn len_width(r: LengthRepr) -> u64 {
    match r {
        LengthRepr::U8 => 1,
        LengthRepr::U16 => 2,
        LengthRepr::U32 => 4,
        _ => unreachable!(),
    }
}
fn tagged_layout<T: ZeroSchemaType>(id: u32) -> Result<Pairs, HarnessError> {
    let mut out = basic_layout::<T>(id)?;
    let b = u64::from(id) * 1000;
    let TypeKind::TaggedUnion {
        tag_layout,
        tag_offset,
        payload_offset,
        payload_size,
        payload_align,
        ..
    } = T::LAYOUT.kind()
    else {
        return Err(HarnessError::InvalidData("expected tagged layout"));
    };
    out.extend([
        (b + 300, u("tag offset", tag_offset)?),
        (b + 301, u("payload offset", payload_offset)?),
        (b + 302, u("payload size", payload_size)?),
        (b + 303, u("payload align", payload_align)?),
    ]);
    let TypeKind::ScalarEnum { repr, endian: e } = tag_layout.kind() else {
        return Err(HarnessError::InvalidData("expected scalar tag"));
    };
    out.extend([
        (b + 304, int_width(repr)),
        (b + 305, u("tag align", tag_layout.align())?),
        (b + 306, endian(e)),
    ]);
    for (j, v) in T::LAYOUT.variants().iter().enumerate() {
        let k = b + 320 + j as u64 * 3;
        out.extend([
            (k, v.raw_tag()),
            (k + 1, u("variant payload size", v.payload_size())?),
            (k + 2, u("variant payload align", v.payload_align())?),
        ]);
    }
    Ok(out)
}
fn external_tagged_layout<T: ZeroSchemaType>(
    id: u32,
    union: &'static LayoutDescriptor,
) -> Result<Pairs, HarnessError> {
    let mut out = basic_layout::<T>(id)?;
    let b = u64::from(id) * 1000;
    let TypeKind::TaggedUnion { tag_layout, .. } = union.kind() else {
        return Err(HarnessError::InvalidData("expected external tagged layout"));
    };
    let tag_field = field(T::LAYOUT, "tag")?;
    let payload_field = field(T::LAYOUT, "payload")?;
    out.extend([
        (b + 300, u("tag offset", tag_field.offset())?),
        (b + 301, u("payload offset", payload_field.offset())?),
        (b + 302, u("payload size", payload_field.size())?),
        (b + 303, u("payload align", payload_field.align())?),
    ]);
    let TypeKind::ScalarEnum { repr, endian: e } = tag_layout.kind() else {
        return Err(HarnessError::InvalidData("expected external scalar tag"));
    };
    out.extend([
        (b + 304, int_width(repr)),
        (b + 305, u("tag align", tag_layout.align())?),
        (b + 306, endian(e)),
    ]);
    for (j, variant) in union.variants().iter().enumerate() {
        let k = b + 320 + j as u64 * 3;
        out.extend([
            (k, variant.raw_tag()),
            (k + 1, u("variant payload size", variant.payload_size())?),
            (k + 2, u("variant payload align", variant.payload_align())?),
        ]);
    }
    Ok(out)
}
fn enum_layout() -> Result<Pairs, HarnessError> {
    let mut out = basic_layout::<ConformanceEnums>(1006)?;
    let layouts = [
        ConformanceEnum8::LAYOUT,
        ConformanceEnumNative16::LAYOUT,
        ConformanceEnumLittle16::LAYOUT,
        ConformanceEnumBig16::LAYOUT,
        ConformanceEnumNative32::LAYOUT,
        ConformanceEnumLittle32::LAYOUT,
        ConformanceEnumBig32::LAYOUT,
    ];
    for (j, l) in layouts.into_iter().enumerate() {
        let TypeKind::ScalarEnum { repr, endian: e } = l.kind() else {
            return Err(HarnessError::InvalidData("expected scalar enum"));
        };
        let k = 1006300 + j as u64 * 5;
        out.extend([
            (k, u("enum size", l.size())?),
            (k + 1, u("enum align", l.align())?),
            (k + 2, int_width(repr)),
            (k + 3, endian(e)),
            (k + 4, l.enum_values()[0].raw_value()),
        ]);
    }
    Ok(out)
}
fn string_layout() -> Result<Pairs, HarnessError> {
    let mut out = basic_layout::<ConformanceStrings>(1007)?;
    for (j, f) in ConformanceStrings::LAYOUT
        .fields()
        .iter()
        .take(12)
        .enumerate()
    {
        let FieldKind::String(s) = f.kind() else {
            return Err(HarnessError::InvalidData("expected string field"));
        };
        let enc = match s.encoding() {
            StringEncoding::Utf8 => 1,
            StringEncoding::CBytes => 2,
            StringEncoding::U16 => 3,
            StringEncoding::U16C => 4,
            _ => unreachable!(),
        };
        let unit = if matches!(s.encoding(), StringEncoding::U16 | StringEncoding::U16C) {
            2
        } else {
            1
        };
        let (length_width, length_endian, length_offset) = s.length().map_or((0, 0, 0), |l| {
            (len_width(l.repr()), endian(l.endian()), l.offset() as u64)
        });
        let k = 1007300 + j as u64 * 8;
        out.extend([
            (k, enc),
            (k + 1, u("string capacity", s.capacity())?),
            (k + 2, unit),
            (k + 3, u("data offset", s.data_offset())?),
            (k + 4, length_width),
            (k + 5, length_endian),
            (k + 6, length_offset),
            (k + 7, if s.tail() == TailPolicy::Zero { 1 } else { 0 }),
        ]);
    }
    Ok(out)
}

macro_rules! codec {
    ($enc:ident,$obs:ident,$ty:ty,$value:expr,$pairs:expr) => {
        fn $enc() -> Result<Vec<u8>, HarnessError> {
            let value: $ty = $value;
            let b = value.encode().map_err(bad)?;
            Ok(b.as_bytes().to_vec())
        }
        fn $obs(bytes: &[u8]) -> Result<Pairs, HarnessError> {
            if bytes.len() != <$ty>::WIRE_SIZE {
                return Err(HarnessError::InvalidByteLength {
                    expected: <$ty>::WIRE_SIZE,
                    actual: bytes.len(),
                });
            }
            let mut b = zero_schema::make_buffer_for!($ty);
            b.as_bytes_mut().copy_from_slice(bytes);
            let v = <$ty>::parse(b.as_bytes()).map_err(bad)?;
            Ok(($pairs)(&v))
        }
    };
}

codec!(
    bytes1,
    obs1,
    ConformanceScalars,
    ConformanceScalars {
        marker: 0xa5,
        little16: 0x0102,
        big32: 0x01020304
    },
    |v: &ConformanceScalars| vec![
        (1001501, v.marker as u64),
        (1001502, v.little16 as u64),
        (1001503, v.big32 as u64)
    ]
);
codec!(
    bytes2,
    obs2,
    ConformanceAligned,
    ConformanceAligned {
        prefix: 0x12,
        value: 0x11223344,
        suffix: 0x34
    },
    |v: &ConformanceAligned| vec![
        (1002501, v.prefix as u64),
        (1002502, v.value as u64),
        (1002503, v.suffix as u64)
    ]
);
codec!(
    bytes3,
    obs3,
    ConformanceMessage,
    ConformanceMessage::Empty,
    |v: &ConformanceMessage| vec![(1003501, v.tag().to_raw().into())]
);
codec!(
    bytes4,
    obs4,
    ConformanceMessage,
    ConformanceMessage::Data(ConformanceData { bits: 0x11223344 }),
    |v: &ConformanceMessage| match v {
        ConformanceMessage::Data(data) => vec![
            (1004501, v.tag().to_raw().into()),
            (1004502, data.bits as u64)
        ],
        _ => unreachable!(),
    }
);
fn strings_value<'a>() -> ConformanceStrings<'a> {
    ConformanceStrings {
        utf8_u8: "A\0B",
        utf8_u16_native: "N",
        utf8_u16_little: "le",
        utf8_u16_big: "BE",
        utf8_u32_native: "n",
        utf8_u32_little: "l",
        utf8_u32_big: "b",
        c_bytes: c"\xff\x7f",
        u16_u8: U16Str::from_slice(&[0xd800]),
        u16_u16: U16Str::from_slice(&[0x41, 0]),
        u16_u32: U16Str::from_slice(&[0xdc00]),
        u16_c: U16CStr::from_slice(&[0xd800, 0]).unwrap(),
        fixed: &[0, 0x7f, 0x80, 0xfe, 0xff],
    }
}
codec!(
    bytes5,
    obs5,
    ConformancePrimitives,
    ConformancePrimitives {
        u8_value: 0xa5,
        i8_bits: 0x81u8 as i8,
        bool_value: true,
        u16_native: 0x1122,
        u16_little: 0x0102,
        u16_big: 0x0102,
        i16_native: 0x8001u16 as i16,
        i16_little: 0x8102u16 as i16,
        i16_big: 0x8102u16 as i16,
        u32_native: 0x11223344,
        u32_little: 0x01020304,
        u32_big: 0x01020304,
        i32_native: 0x80000001u32 as i32,
        i32_little: 0x81020304u32 as i32,
        i32_big: 0x81020304u32 as i32,
        u64_native: 0x1122334455667788,
        u64_little: 0x0102030405060708,
        u64_big: 0x0102030405060708,
        i64_native: 0x8000000000000001u64 as i64,
        i64_little: 0x8102030405060708u64 as i64,
        i64_big: 0x8102030405060708u64 as i64,
        f32_native: f32::from_bits(0x80000000),
        f32_little: f32::from_bits(0x7fc01234),
        f32_big: f32::from_bits(0x7fc01234),
        f64_native: f64::from_bits(0x8000000000000000),
        f64_little: f64::from_bits(0x7ff8000000001234),
        f64_big: f64::from_bits(0x7ff8000000001234)
    },
    |v: &ConformancePrimitives| {
        let a = [
            v.u8_value as u64,
            v.i8_bits as u8 as u64,
            v.bool_value as u64,
            v.u16_native as u64,
            v.u16_little as u64,
            v.u16_big as u64,
            v.i16_native as u16 as u64,
            v.i16_little as u16 as u64,
            v.i16_big as u16 as u64,
            v.u32_native as u64,
            v.u32_little as u64,
            v.u32_big as u64,
            v.i32_native as u32 as u64,
            v.i32_little as u32 as u64,
            v.i32_big as u32 as u64,
            v.u64_native,
            v.u64_little,
            v.u64_big,
            v.i64_native as u64,
            v.i64_little as u64,
            v.i64_big as u64,
            v.f32_native.to_bits() as u64,
            v.f32_little.to_bits() as u64,
            v.f32_big.to_bits() as u64,
            v.f64_native.to_bits(),
            v.f64_little.to_bits(),
            v.f64_big.to_bits(),
        ];
        a.into_iter()
            .enumerate()
            .map(|(i, x)| (1005501 + i as u64, x))
            .collect()
    }
);
codec!(
    bytes6,
    obs6,
    ConformanceEnums,
    ConformanceEnums {
        enum8: ConformanceEnum8::r#type,
        native16: ConformanceEnumNative16::Value,
        little16: ConformanceEnumLittle16::Value,
        big16: ConformanceEnumBig16::Value,
        native32: ConformanceEnumNative32::Value,
        little32: ConformanceEnumLittle32::Value,
        big32: ConformanceEnumBig32::Value
    },
    |v: &ConformanceEnums| {
        [
            v.enum8.to_raw().into(),
            v.native16.to_raw().into(),
            v.little16.to_raw().into(),
            v.big16.to_raw().into(),
            v.native32.to_raw().into(),
            v.little32.to_raw().into(),
            v.big32.to_raw().into(),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, x)| (1006501 + i as u64, x))
        .collect()
    }
);
codec!(
    bytes7,
    obs7,
    ConformanceStrings<'_>,
    strings_value(),
    |v: &ConformanceStrings<'_>| {
        let mut o = vec![];
        let mut push = |x| {
            let k = 1007501 + o.len() as u64;
            o.push((k, x));
        };
        push(v.utf8_u8.len() as u64);
        for x in v.utf8_u8.as_bytes() {
            push(*x as u64)
        }
        push(v.utf8_u16_native.len() as u64);
        for x in v.utf8_u16_native.as_bytes() {
            push(*x as u64)
        }
        push(v.utf8_u16_little.len() as u64);
        for x in v.utf8_u16_little.as_bytes() {
            push(*x as u64)
        }
        push(v.utf8_u16_big.len() as u64);
        for x in v.utf8_u16_big.as_bytes() {
            push(*x as u64)
        }
        for s in [v.utf8_u32_native, v.utf8_u32_little, v.utf8_u32_big] {
            push(s.len() as u64);
            push(s.as_bytes()[0] as u64)
        }
        let c = v.c_bytes.to_bytes_with_nul();
        push(c.len() as u64);
        for x in c {
            push(*x as u64)
        }
        for s in [v.u16_u8, v.u16_u16, v.u16_u32] {
            push(s.len() as u64);
            for x in s.as_slice() {
                push(*x as u64)
            }
        }
        let c = v.u16_c.as_slice_with_nul();
        push(c.len() as u64);
        for x in c {
            push(*x as u64)
        }
        for x in v.fixed {
            push(*x as u64)
        }
        o
    }
);
codec!(
    bytes8,
    obs8,
    ConformanceNested,
    ConformanceNested {
        prefix: 0x5a,
        child: ConformanceScalars {
            marker: 0xa5,
            little16: 0x0102,
            big32: 0x01020304
        },
        suffix: 0x7788
    },
    |v: &ConformanceNested| vec![
        (1008501, v.prefix as u64),
        (1008502, v.child.marker as u64),
        (1008503, v.child.little16 as u64),
        (1008504, v.child.big32 as u64),
        (1008505, v.suffix as u64)
    ]
);
codec!(
    bytes10,
    obs10,
    ConformanceExternalMessage,
    ConformanceExternalMessage {
        prefix: 0xc1,
        tag: ConformanceTag::Data,
        payload: ConformanceMessage::Data(ConformanceData { bits: 0x55667788 }),
        suffix: 0x99aa
    },
    |v: &ConformanceExternalMessage| match &v.payload {
        ConformanceMessage::Data(d) => vec![
            (1010501, v.prefix as u64),
            (1010502, v.tag.to_raw().into()),
            (1010503, v.payload.tag().to_raw().into()),
            (1010504, d.bits as u64),
            (1010505, v.suffix as u64)
        ],
        _ => unreachable!(),
    }
);
codec!(
    bytes11,
    obs11,
    ConformanceExternalUnits,
    ConformanceExternalUnits {
        prefix: 0xd1,
        tag: ConformanceUnitTag::B,
        payload: ConformanceUnits::B,
        suffix: 0xbbcc
    },
    |v: &ConformanceExternalUnits| vec![
        (1011501, v.prefix as u64),
        (1011502, v.tag.to_raw().into()),
        (1011503, v.payload.tag().to_raw().into()),
        (1011504, v.suffix as u64)
    ]
);

#[repr(C, align(8))]
struct Z8;
#[repr(C, align(16))]
struct Z16;
#[repr(C, align(32))]
struct Z32;
#[repr(C)]
struct ZstLayout {
    leading: Z8,
    first: u8,
    interleaved: Z16,
    word: u32,
    trailing: Z32,
}
fn layout9() -> Result<Pairs, HarnessError> {
    let b = 1009000;
    let vals = [
        size_of::<ZstLayout>(),
        align_of::<ZstLayout>(),
        size_of::<ZstLayout>(),
        offset_of!(ZstLayout, leading),
        size_of::<Z8>(),
        align_of::<Z8>(),
        offset_of!(ZstLayout, first),
        1,
        1,
        offset_of!(ZstLayout, interleaved),
        0,
        align_of::<Z16>(),
        offset_of!(ZstLayout, word),
        4,
        4,
        offset_of!(ZstLayout, trailing),
        0,
        align_of::<Z32>(),
    ];
    let keys = [
        b + 1,
        b + 2,
        b + 3,
        b + 10,
        b + 11,
        b + 12,
        b + 13,
        b + 14,
        b + 15,
        b + 16,
        b + 17,
        b + 18,
        b + 19,
        b + 20,
        b + 21,
        b + 22,
        b + 23,
        b + 24,
    ];
    Ok(keys
        .into_iter()
        .zip(vals)
        .map(|(k, v)| (k, v as u64))
        .collect())
}
fn bytes9() -> Result<Vec<u8>, HarnessError> {
    let mut v = vec![0; size_of::<ZstLayout>()];
    v[offset_of!(ZstLayout, first)] = 0xa5;
    v[offset_of!(ZstLayout, word)..offset_of!(ZstLayout, word) + 4]
        .copy_from_slice(&0x11223344u32.to_ne_bytes());
    Ok(v)
}
macro_rules! layout_fn {
    ($name:ident,$ty:ty,$id:literal) => {
        fn $name() -> Result<Pairs, HarnessError> {
            basic_layout::<$ty>($id)
        }
    };
}
layout_fn!(layout1, ConformanceScalars, 1001);
layout_fn!(layout2, ConformanceAligned, 1002);
fn layout3() -> Result<Pairs, HarnessError> {
    tagged_layout::<ConformanceMessage>(1003)
}
fn layout4() -> Result<Pairs, HarnessError> {
    tagged_layout::<ConformanceMessage>(1004)
}
layout_fn!(layout5, ConformancePrimitives, 1005);
layout_fn!(layout8, ConformanceNested, 1008);
fn layout10() -> Result<Pairs, HarnessError> {
    external_tagged_layout::<ConformanceExternalMessage>(1010, ConformanceMessage::LAYOUT)
}
fn layout11() -> Result<Pairs, HarnessError> {
    external_tagged_layout::<ConformanceExternalUnits>(1011, ConformanceUnits::LAYOUT)
}
fn obs9(x: &[u8]) -> Result<Pairs, HarnessError> {
    if x.len() != size_of::<ZstLayout>() {
        return Err(HarnessError::InvalidByteLength {
            expected: size_of::<ZstLayout>(),
            actual: x.len(),
        });
    }
    let o = offset_of!(ZstLayout, word);
    let word = u32::from_ne_bytes(x[o..o + 4].try_into().unwrap());
    Ok(vec![
        (1009501, x[offset_of!(ZstLayout, first)] as u64),
        (1009502, word as u64),
    ])
}
pub(crate) static CASES: &[Case] = &[
    Case {
        case_id: 1001,
        root_id: "conformance-scalars",
        expected_key_values: layout1,
        rust_bytes: bytes1,
        rust_observe: obs1,
    },
    Case {
        case_id: 1002,
        root_id: "conformance-aligned",
        expected_key_values: layout2,
        rust_bytes: bytes2,
        rust_observe: obs2,
    },
    Case {
        case_id: 1003,
        root_id: "conformance-message-empty",
        expected_key_values: layout3,
        rust_bytes: bytes3,
        rust_observe: obs3,
    },
    Case {
        case_id: 1004,
        root_id: "conformance-message-data",
        expected_key_values: layout4,
        rust_bytes: bytes4,
        rust_observe: obs4,
    },
    Case {
        case_id: 1005,
        root_id: "conformance-primitives",
        expected_key_values: layout5,
        rust_bytes: bytes5,
        rust_observe: obs5,
    },
    Case {
        case_id: 1006,
        root_id: "conformance-enums",
        expected_key_values: enum_layout,
        rust_bytes: bytes6,
        rust_observe: obs6,
    },
    Case {
        case_id: 1007,
        root_id: "conformance-strings",
        expected_key_values: string_layout,
        rust_bytes: bytes7,
        rust_observe: obs7,
    },
    Case {
        case_id: 1008,
        root_id: "conformance-nested",
        expected_key_values: layout8,
        rust_bytes: bytes8,
        rust_observe: obs8,
    },
    Case {
        case_id: 1009,
        root_id: "conformance-zst-layout",
        expected_key_values: layout9,
        rust_bytes: bytes9,
        rust_observe: obs9,
    },
    Case {
        case_id: 1010,
        root_id: "conformance-external-message",
        expected_key_values: layout10,
        rust_bytes: bytes10,
        rust_observe: obs10,
    },
    Case {
        case_id: 1011,
        root_id: "conformance-external-units",
        expected_key_values: layout11,
        rust_bytes: bytes11,
        rust_observe: obs11,
    },
];
