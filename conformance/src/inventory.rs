use zero_schema::{
    Endian, FieldKind, IntegerRepr, LayoutDescriptor, LengthRepr, StringEncoding, TypeKind,
};
use zero_schema_schema_corpus::conformance::*;

use crate::report::HarnessError;

type Pairs = Vec<(u64, u64)>;
type RustMutation = fn(&[u8]) -> Result<Vec<u8>, HarnessError>;

fn release<T>(value: T) {
    drop(value);
}

pub(crate) struct Case {
    pub(crate) case_id: u32,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) root_id: &'static str,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) expected_key_values: fn() -> Result<Pairs, HarnessError>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) schema_size: fn() -> usize,
    pub(crate) rust_observe: fn(&[u8]) -> Result<Pairs, HarnessError>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) rust_mutate: Option<RustMutation>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) mutated_observations: &'static [(u64, u64)],
}

fn size(what: &'static str, value: usize) -> Result<u64, HarnessError> {
    u64::try_from(value).map_err(|_| HarnessError::ValueOutOfRange { what, value })
}

fn incoming(expected: usize, actual: usize) -> Result<(), HarnessError> {
    if actual == expected {
        Ok(())
    } else {
        Err(HarnessError::InvalidByteLength { expected, actual })
    }
}

fn field<'a>(
    layout: &'a LayoutDescriptor,
    name: &str,
) -> Result<&'a zero_schema::FieldDescriptor, HarnessError> {
    layout
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .ok_or(HarnessError::InvalidData("missing layout field"))
}

fn basic_layout(
    id: u32,
    size_bytes: usize,
    align: usize,
    stride: usize,
    layout: &'static LayoutDescriptor,
) -> Result<Pairs, HarnessError> {
    let base = u64::from(id) * 1000;
    let mut pairs = vec![
        (base + 1, size("schema size", size_bytes)?),
        (base + 2, size("schema alignment", align)?),
        (base + 3, size("schema stride", stride)?),
    ];
    for (index, field) in layout.fields().iter().enumerate() {
        let key = base + 10 + u64::try_from(index).expect("field index fits") * 3;
        pairs.extend([
            (key, size("field offset", field.offset())?),
            (key + 1, size("field size", field.size())?),
            (key + 2, size("field alignment", field.align())?),
        ]);
    }
    Ok(pairs)
}

fn endian(endian: Endian) -> u64 {
    match endian {
        Endian::Native => 0,
        Endian::Little => 1,
        Endian::Big => 2,
        _ => unreachable!(),
    }
}

fn integer_width(repr: IntegerRepr) -> u64 {
    match repr {
        IntegerRepr::U8 => 1,
        IntegerRepr::U16 => 2,
        IntegerRepr::U32 => 4,
        _ => unreachable!(),
    }
}

fn length_width(repr: LengthRepr) -> u64 {
    match repr {
        LengthRepr::U8 => 1,
        LengthRepr::U16 => 2,
        LengthRepr::U32 => 4,
        _ => unreachable!(),
    }
}

fn external_tagged_layout(
    id: u32,
    size_bytes: usize,
    align: usize,
    stride: usize,
    layout: &'static LayoutDescriptor,
    payload_name: &str,
) -> Result<Pairs, HarnessError> {
    let mut pairs = basic_layout(id, size_bytes, align, stride, layout)?;
    let base = u64::from(id) * 1000;
    let payload_field = field(layout, payload_name)?;
    let FieldKind::ExternalTaggedUnion { payload, tag } = payload_field.kind() else {
        return Err(HarnessError::InvalidData("missing external union metadata"));
    };
    let TypeKind::ScalarEnum {
        repr,
        endian: tag_endian,
    } = tag.layout().kind()
    else {
        return Err(HarnessError::InvalidData(
            "external tag is not a scalar enum",
        ));
    };
    pairs.extend([
        (base + 300, size("tag offset", tag.offset())?),
        (base + 301, size("payload offset", payload_field.offset())?),
        (base + 302, size("payload size", payload_field.size())?),
        (
            base + 303,
            size("payload alignment", payload_field.align())?,
        ),
        (base + 304, integer_width(repr)),
        (base + 305, size("tag alignment", tag.layout().align())?),
        (base + 306, endian(tag_endian)),
    ]);
    for (index, variant) in payload.variants().iter().enumerate() {
        let key = base + 320 + u64::try_from(index).expect("variant index fits") * 3;
        pairs.extend([
            (key, variant.raw_tag()),
            (
                key + 1,
                size("variant payload size", variant.payload_size())?,
            ),
            (
                key + 2,
                size("variant payload alignment", variant.payload_align())?,
            ),
        ]);
    }
    Ok(pairs)
}

fn layout1() -> Result<Pairs, HarnessError> {
    basic_layout(
        1001,
        ConformanceScalars::SCHEMA_SIZE,
        ConformanceScalars::SCHEMA_ALIGN,
        ConformanceScalars::SCHEMA_STRIDE,
        ConformanceScalars::LAYOUT,
    )
}
fn layout2() -> Result<Pairs, HarnessError> {
    basic_layout(
        1002,
        ConformanceAligned::SCHEMA_SIZE,
        ConformanceAligned::SCHEMA_ALIGN,
        ConformanceAligned::SCHEMA_STRIDE,
        ConformanceAligned::LAYOUT,
    )
}
fn layout3() -> Result<Pairs, HarnessError> {
    external_tagged_layout(
        1003,
        ConformanceMessageRecord::SCHEMA_SIZE,
        ConformanceMessageRecord::SCHEMA_ALIGN,
        ConformanceMessageRecord::SCHEMA_STRIDE,
        ConformanceMessageRecord::LAYOUT,
        "payload",
    )
}
fn layout4() -> Result<Pairs, HarnessError> {
    external_tagged_layout(
        1004,
        ConformanceMessageRecord::SCHEMA_SIZE,
        ConformanceMessageRecord::SCHEMA_ALIGN,
        ConformanceMessageRecord::SCHEMA_STRIDE,
        ConformanceMessageRecord::LAYOUT,
        "payload",
    )
}
fn layout5() -> Result<Pairs, HarnessError> {
    basic_layout(
        1005,
        ConformancePrimitives::SCHEMA_SIZE,
        ConformancePrimitives::SCHEMA_ALIGN,
        ConformancePrimitives::SCHEMA_STRIDE,
        ConformancePrimitives::LAYOUT,
    )
}
fn enum_layout() -> Result<Pairs, HarnessError> {
    let mut pairs = basic_layout(
        1006,
        ConformanceEnums::SCHEMA_SIZE,
        ConformanceEnums::SCHEMA_ALIGN,
        ConformanceEnums::SCHEMA_STRIDE,
        ConformanceEnums::LAYOUT,
    )?;
    let layouts = [
        ConformanceEnum8::LAYOUT,
        ConformanceEnumNative16::LAYOUT,
        ConformanceEnumLittle16::LAYOUT,
        ConformanceEnumBig16::LAYOUT,
        ConformanceEnumNative32::LAYOUT,
        ConformanceEnumLittle32::LAYOUT,
        ConformanceEnumBig32::LAYOUT,
    ];
    for (index, layout) in layouts.into_iter().enumerate() {
        let TypeKind::ScalarEnum {
            repr,
            endian: enum_endian,
        } = layout.kind()
        else {
            return Err(HarnessError::InvalidData("missing scalar enum metadata"));
        };
        let key = 1006300 + u64::try_from(index).expect("enum index fits") * 5;
        pairs.extend([
            (key, size("enum size", layout.size())?),
            (key + 1, size("enum alignment", layout.align())?),
            (key + 2, integer_width(repr)),
            (key + 3, endian(enum_endian)),
            (key + 4, layout.enum_values()[0].raw_value()),
        ]);
    }
    Ok(pairs)
}
fn string_layout() -> Result<Pairs, HarnessError> {
    let mut pairs = basic_layout(
        1007,
        ConformanceStrings::SCHEMA_SIZE,
        ConformanceStrings::SCHEMA_ALIGN,
        ConformanceStrings::SCHEMA_STRIDE,
        ConformanceStrings::LAYOUT,
    )?;
    for (index, field) in ConformanceStrings::LAYOUT
        .fields()
        .iter()
        .take(12)
        .enumerate()
    {
        let FieldKind::String(string) = field.kind() else {
            return Err(HarnessError::InvalidData("expected string field metadata"));
        };
        let encoding = match string.encoding() {
            StringEncoding::Utf8 => 1,
            StringEncoding::CBytes => 2,
            StringEncoding::U16 => 3,
            StringEncoding::U16C => 4,
            _ => unreachable!(),
        };
        let unit_width = if matches!(
            string.encoding(),
            StringEncoding::U16 | StringEncoding::U16C
        ) {
            2
        } else {
            1
        };
        let (width, byte_order, offset) = string.length().map_or((0, 0, 0), |length| {
            (
                length_width(length.repr()),
                endian(length.endian()),
                length.offset() as u64,
            )
        });
        let key = 1007300 + u64::try_from(index).expect("string index fits") * 8;
        pairs.extend([
            (key, encoding),
            (key + 1, size("string capacity", string.capacity())?),
            (key + 2, unit_width),
            (key + 3, size("string data offset", string.data_offset())?),
            (key + 4, width),
            (key + 5, byte_order),
            (key + 6, offset),
        ]);
    }
    Ok(pairs)
}
fn layout8() -> Result<Pairs, HarnessError> {
    let mut pairs = basic_layout(
        1008,
        ConformanceNested::SCHEMA_SIZE,
        ConformanceNested::SCHEMA_ALIGN,
        ConformanceNested::SCHEMA_STRIDE,
        ConformanceNested::LAYOUT,
    )?;
    let samples = field(ConformanceNested::LAYOUT, "samples")?;
    let FieldKind::Array(array) = samples.kind() else {
        return Err(HarnessError::InvalidData("missing samples array metadata"));
    };
    pairs.extend([
        (1008300, size("array length", array.length())?),
        (1008301, size("array stride", array.stride())?),
    ]);
    Ok(pairs)
}
fn layout10() -> Result<Pairs, HarnessError> {
    external_tagged_layout(
        1010,
        ConformanceExternalMessage::SCHEMA_SIZE,
        ConformanceExternalMessage::SCHEMA_ALIGN,
        ConformanceExternalMessage::SCHEMA_STRIDE,
        ConformanceExternalMessage::LAYOUT,
        "payload",
    )
}
fn layout11() -> Result<Pairs, HarnessError> {
    external_tagged_layout(
        1011,
        ConformanceExternalUnits::SCHEMA_SIZE,
        ConformanceExternalUnits::SCHEMA_ALIGN,
        ConformanceExternalUnits::SCHEMA_STRIDE,
        ConformanceExternalUnits::LAYOUT,
        "payload",
    )
}
#[cfg(test)]
pub(crate) fn native_option_layout() -> Result<Pairs, HarnessError> {
    let mut pairs = basic_layout(
        1012,
        ConformanceOptions::SCHEMA_SIZE,
        ConformanceOptions::SCHEMA_ALIGN,
        ConformanceOptions::SCHEMA_STRIDE,
        ConformanceOptions::LAYOUT,
    )?;
    for (span_key, optional_key, name) in [
        (1012300, 1012301, "maybe_kind"),
        (1012302, 1012303, "maybe_child"),
        (1012304, 1012305, "maybe_array"),
    ] {
        let descriptor = field(ConformanceOptions::LAYOUT, name)?;
        if !descriptor.is_optional() {
            return Err(HarnessError::InvalidData("missing optional field metadata"));
        }
        pairs.extend([
            (span_key, size("optional sentinel span", descriptor.size())?),
            (optional_key, u64::from(descriptor.is_optional())),
        ]);
    }
    let descriptor = field(ConformanceOptions::LAYOUT, "maybe_array")?;
    let FieldKind::Array(array) = descriptor.kind() else {
        return Err(HarnessError::InvalidData("missing optional array metadata"));
    };
    pairs.extend([
        (1012306, size("optional array length", array.length())?),
        (1012307, size("optional array stride", array.stride())?),
    ]);
    Ok(pairs)
}

fn obs1(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceScalars::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceScalars);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceScalars::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1001501, u64::from(view.marker())),
        (1001502, u64::from(view.little16())),
        (1001503, u64::from(view.big32())),
    ])
}
fn obs2(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceAligned::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceAligned);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceAligned::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1002501, u64::from(view.prefix())),
        (1002502, u64::from(view.value())),
        (1002503, u64::from(view.suffix())),
    ])
}
fn obs3(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceMessageRecord::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceMessageRecord);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceMessageRecord::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![(1003501, view.payload().tag() as u64)])
}
fn obs4(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceMessageRecord::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceMessageRecord);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceMessageRecord::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1004501, view.payload().tag() as u64),
        (
            1004502,
            u64::from(view.payload().data().expect("Data selected").bits()),
        ),
    ])
}
fn obs5(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformancePrimitives::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformancePrimitives);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformancePrimitives::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    let values = [
        view.u8_value() as u64,
        view.i8_bits() as u8 as u64,
        view.bool_value() as u64,
        view.u16_native() as u64,
        view.u16_little() as u64,
        view.u16_big() as u64,
        view.i16_native() as u16 as u64,
        view.i16_little() as u16 as u64,
        view.i16_big() as u16 as u64,
        view.u32_native() as u64,
        view.u32_little() as u64,
        view.u32_big() as u64,
        view.i32_native() as u32 as u64,
        view.i32_little() as u32 as u64,
        view.i32_big() as u32 as u64,
        view.u64_native(),
        view.u64_little(),
        view.u64_big(),
        view.i64_native() as u64,
        view.i64_little() as u64,
        view.i64_big() as u64,
        view.f32_native().to_bits() as u64,
        view.f32_little().to_bits() as u64,
        view.f32_big().to_bits() as u64,
        view.f64_native().to_bits(),
        view.f64_little().to_bits(),
        view.f64_big().to_bits(),
    ];
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, value)| (1005501 + index as u64, value))
        .collect())
}
fn obs6(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceEnums::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceEnums);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceEnums::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1006501, view.enum8() as u8 as u64),
        (1006502, view.native16() as u16 as u64),
        (1006503, view.little16() as u16 as u64),
        (1006504, view.big16() as u16 as u64),
        (1006505, view.native32() as u32 as u64),
        (1006506, view.little32() as u32 as u64),
        (1006507, view.big32() as u32 as u64),
    ])
}
fn obs7(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceStrings::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceStrings<'static>);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceStrings::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    let mut pairs = Vec::new();
    let mut push = |value| {
        let key = 1007501 + pairs.len() as u64;
        pairs.push((key, value));
    };
    push(view.utf8_u8().len() as u64);
    for byte in view.utf8_u8().as_bytes() {
        push(u64::from(*byte));
    }
    for string in [
        view.utf8_u16_native(),
        view.utf8_u16_little(),
        view.utf8_u16_big(),
    ] {
        push(string.len() as u64);
        for byte in string.as_bytes() {
            push(u64::from(*byte));
        }
    }
    for string in [
        view.utf8_u32_native(),
        view.utf8_u32_little(),
        view.utf8_u32_big(),
    ] {
        push(string.len() as u64);
        push(u64::from(string.as_bytes()[0]));
    }
    let c_bytes = view.c_bytes().to_bytes_with_nul();
    push(c_bytes.len() as u64);
    for byte in c_bytes {
        push(u64::from(*byte));
    }
    for string in [view.u16_u8(), view.u16_u16(), view.u16_u32()] {
        push(string.len() as u64);
        for unit in string.as_slice() {
            push(u64::from(*unit));
        }
    }
    let u16_c = view.u16_c().as_slice_with_nul();
    push(u16_c.len() as u64);
    for unit in u16_c {
        push(u64::from(*unit));
    }
    for byte in view.fixed() {
        push(u64::from(*byte));
    }
    Ok(pairs)
}
fn obs8(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceNested::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceNested);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceNested::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1008501, u64::from(view.prefix())),
        (1008502, u64::from(view.child().marker())),
        (1008503, u64::from(view.child().little16())),
        (1008504, u64::from(view.child().big32())),
        (1008505, u64::from(view.samples().get(0).expect("sample 0"))),
        (1008506, u64::from(view.samples().get(1).expect("sample 1"))),
        (1008507, u64::from(view.samples().get(2).expect("sample 2"))),
        (1008508, u64::from(view.suffix())),
    ])
}
fn obs10(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceExternalMessage::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceExternalMessage);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceExternalMessage::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1010501, u64::from(view.prefix())),
        (1010502, view.tag() as u64),
        (1010503, view.payload().tag() as u64),
        (
            1010504,
            u64::from(view.payload().data().expect("Data selected").bits()),
        ),
        (1010505, u64::from(view.suffix())),
    ])
}
fn obs11(bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceExternalUnits::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceExternalUnits);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceExternalUnits::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust access"))?;
    Ok(vec![
        (1011501, u64::from(view.prefix())),
        (1011502, view.tag() as u64),
        (1011503, view.payload().tag() as u64),
        (1011504, u64::from(view.suffix())),
    ])
}
#[cfg(test)]
pub(crate) fn native_option_observe(case_id: u32, bytes: &[u8]) -> Result<Pairs, HarnessError> {
    incoming(ConformanceOptions::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceOptions);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let view = ConformanceOptions::access(storage.as_bytes())
        .map_err(|_| HarnessError::InvalidData("C++ option bytes failed Rust access"))?;
    let key = u64::from(case_id) * 1000 + 501;
    Ok(vec![
        (key, u64::from(view.maybe_kind().is_some())),
        (key + 1, u64::from(view.maybe_child().is_some())),
        (key + 2, u64::from(view.maybe_array().is_some())),
    ])
}

#[cfg(test)]
pub(crate) fn native_option_clear_child(bytes: &[u8]) -> Result<Vec<u8>, HarnessError> {
    incoming(ConformanceOptions::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceOptions);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let mut view = ConformanceOptions::access_mut(storage.as_bytes_mut())
        .map_err(|_| HarnessError::InvalidData("C++ option bytes failed Rust mutable access"))?;
    view.maybe_child_mut()
        .set(None)
        .map_err(|_| HarnessError::InvalidData("Rust optional clear failed"))?;
    release(view);
    Ok(storage.as_bytes().to_vec())
}

fn mutate1(bytes: &[u8]) -> Result<Vec<u8>, HarnessError> {
    incoming(ConformanceScalars::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceScalars);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let mut view = ConformanceScalars::access_mut(storage.as_bytes_mut())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust mutable access"))?;
    view.marker_mut()
        .set(0x5a)
        .map_err(|_| HarnessError::InvalidData("constrained Rust mutation failed"))?;
    release(view);
    Ok(storage.as_bytes().to_vec())
}
fn mutate10(bytes: &[u8]) -> Result<Vec<u8>, HarnessError> {
    incoming(ConformanceExternalMessage::SCHEMA_SIZE, bytes.len())?;
    let mut storage = zero_schema::schema_buffer!(ConformanceExternalMessage);
    storage.as_bytes_mut().copy_from_slice(bytes);
    let mut view = ConformanceExternalMessage::access_mut(storage.as_bytes_mut())
        .map_err(|_| HarnessError::InvalidData("C++ producer bytes failed Rust mutable access"))?;
    view.prefix_mut()
        .set(0x5a)
        .map_err(|_| HarnessError::InvalidData("constrained Rust mutation failed"))?;
    view.payload_mut()
        .data_mut()
        .expect("C++ producer selects Data")
        .bits_mut()
        .set(0xa1b2_c3d4)
        .map_err(|_| HarnessError::InvalidData("constrained Rust mutation failed"))?;
    view.suffix_mut()
        .set(0x5566)
        .map_err(|_| HarnessError::InvalidData("constrained Rust mutation failed"))?;
    release(view);
    Ok(storage.as_bytes().to_vec())
}

pub(crate) static CASES: &[Case] = &[
    Case {
        case_id: 1001,
        root_id: "conformance-scalars",
        expected_key_values: layout1,
        schema_size: || ConformanceScalars::SCHEMA_SIZE,
        rust_observe: obs1,
        rust_mutate: Some(mutate1),
        mutated_observations: &[(1001501, 0x5a), (1001502, 0x0102), (1001503, 0x0102_0304)],
    },
    Case {
        case_id: 1002,
        root_id: "conformance-aligned",
        expected_key_values: layout2,
        schema_size: || ConformanceAligned::SCHEMA_SIZE,
        rust_observe: obs2,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1003,
        root_id: "conformance-message-empty",
        expected_key_values: layout3,
        schema_size: || ConformanceMessageRecord::SCHEMA_SIZE,
        rust_observe: obs3,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1004,
        root_id: "conformance-message-data",
        expected_key_values: layout4,
        schema_size: || ConformanceMessageRecord::SCHEMA_SIZE,
        rust_observe: obs4,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1005,
        root_id: "conformance-primitives",
        expected_key_values: layout5,
        schema_size: || ConformancePrimitives::SCHEMA_SIZE,
        rust_observe: obs5,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1006,
        root_id: "conformance-enums",
        expected_key_values: enum_layout,
        schema_size: || ConformanceEnums::SCHEMA_SIZE,
        rust_observe: obs6,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1007,
        root_id: "conformance-strings",
        expected_key_values: string_layout,
        schema_size: || ConformanceStrings::SCHEMA_SIZE,
        rust_observe: obs7,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1008,
        root_id: "conformance-nested",
        expected_key_values: layout8,
        schema_size: || ConformanceNested::SCHEMA_SIZE,
        rust_observe: obs8,
        rust_mutate: None,
        mutated_observations: &[],
    },
    Case {
        case_id: 1010,
        root_id: "conformance-external-message",
        expected_key_values: layout10,
        schema_size: || ConformanceExternalMessage::SCHEMA_SIZE,
        rust_observe: obs10,
        rust_mutate: Some(mutate10),
        mutated_observations: &[
            (1010501, 0x5a),
            (1010502, 11),
            (1010503, 11),
            (1010504, 0xa1b2_c3d4),
            (1010505, 0x5566),
        ],
    },
    Case {
        case_id: 1011,
        root_id: "conformance-external-units",
        expected_key_values: layout11,
        schema_size: || ConformanceExternalUnits::SCHEMA_SIZE,
        rust_observe: obs11,
        rust_mutate: None,
        mutated_observations: &[],
    },
];
