use zero_schema as zs;
#[cfg(not(miri))]
use zero_schema_cross_crate_child::BorrowedChild;
use zero_schema_cross_crate_child::{
    BigCode, ChildMessage, DirectChild, GenericBytes, LittleCode, NativeCode, TrailingProjection,
};
use zs::{ErrorKind, ErrorPathSegment, LayoutError, SchemaError, ZeroSchema};

#[derive(Debug, Eq, PartialEq)]
pub struct ErrorFacts {
    pub display: String,
    pub kind: ErrorKind,
    pub schema: &'static str,
    pub segment: Option<ErrorPathSegment>,
    pub source: Option<LayoutError>,
    pub child_schema: Option<&'static str>,
    pub child_source_identical: bool,
}

fn facts<E: SchemaError>(error: E) -> ErrorFacts {
    let child = error.child();
    let source_error = error.source();
    ErrorFacts {
        display: error.to_string(),
        kind: error.kind(),
        schema: error.schema(),
        segment: error.segment(),
        source: source_error
            .and_then(|source| source.downcast_ref::<LayoutError>())
            .copied(),
        child_schema: child.map(SchemaError::schema),
        child_source_identical: match (child, source_error) {
            (Some(child), Some(source)) => core::ptr::eq(
                child as *const dyn SchemaError as *const (),
                source as *const dyn std::error::Error as *const (),
            ),
            _ => false,
        },
    }
}

pub fn encode_big(value: BigCode) -> [u8; 2] {
    value.encode().unwrap().as_bytes().try_into().unwrap()
}

pub fn encode_little(value: LittleCode) -> [u8; 4] {
    value.encode().unwrap().as_bytes().try_into().unwrap()
}

pub fn encode_native(value: NativeCode) -> [u8; 4] {
    value.encode().unwrap().as_bytes().try_into().unwrap()
}

pub fn parse_big(bytes: &[u8]) -> Result<u16, ErrorFacts> {
    let mut buffer = zs::make_buffer_for!(BigCode);
    if bytes.len() == buffer.as_bytes().len() {
        buffer.as_bytes_mut().copy_from_slice(bytes);
    } else {
        return BigCode::parse(bytes).map(|_| unreachable!()).map_err(facts);
    }
    BigCode::parse(buffer.as_bytes())
        .map(|value| match value {
            BigCode::Ready => 0x0102,
            BigCode::r#type => 0xabcd,
        })
        .map_err(facts)
}

pub fn parse_little_prefix(bytes: &[u8]) -> Result<(u32, &[u8]), ErrorFacts> {
    LittleCode::parse_prefix(bytes)
        .map(|(value, rest)| {
            (
                match value {
                    LittleCode::First => 0x0102_0304,
                    LittleCode::Last => 0xffff_fffe,
                },
                rest,
            )
        })
        .map_err(facts)
}

pub fn big_unknown_facts(bytes: &[u8]) -> ErrorFacts {
    let mut buffer = zs::make_buffer_for!(BigCode);
    buffer.as_bytes_mut().copy_from_slice(bytes);
    BigCode::parse(buffer.as_bytes())
        .err()
        .map(facts)
        .expect("unknown scalar value")
}

pub fn big_layout_facts(bytes: &[u8]) -> ErrorFacts {
    BigCode::parse(bytes)
        .err()
        .map(facts)
        .expect("scalar layout error")
}
#[derive(ZeroSchema)]
#[zero(crate = crate::zs)]
struct PrivateChild {
    valid: bool,
    code: BigCode,
}

#[derive(ZeroSchema)]
#[zero(crate = crate::zs)]
pub struct PublicPrivateParent {
    prefix: u8,
    child: PrivateChild,
}

#[derive(ZeroSchema)]
#[zero(crate = crate::zs)]
pub struct PublicParent {
    prefix: u8,
    child: DirectChild,
}

// The dedicated Miri target never exercises this fixture. Miri cannot compile its
// foreign lifetime-erased associated-wire layout; normal rustc still builds it.
#[cfg(not(miri))]
#[derive(ZeroSchema)]
#[zero(crate = crate::zs, borrow = 'a)]
pub struct BorrowingParent<'a> {
    child: BorrowedChild<'a>,
    marker: &'a [u8; 2],
}

#[derive(ZeroSchema)]
#[zero(crate = crate::zs)]
pub struct GenericParent<'a, const N: usize> {
    child: GenericBytes<'a, N>,
    trailing: u8,
}

#[derive(ZeroSchema)]
#[zero(crate = crate::zs)]
pub struct CompositionParent {
    direct: DirectChild,
    projected: TrailingProjection,
    tagged: ChildMessage,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    pub prefix: u8,
    pub valid: bool,
}

#[cfg(not(miri))]
pub fn roundtrip_private(prefix: u8, valid: bool) -> (Vec<u8>, Observation) {
    let value = PublicParent {
        prefix,
        child: DirectChild {
            valid,
            value: 0x2468,
        },
    };
    let buffer = value.encode().unwrap();
    let bytes = buffer.as_bytes().to_vec();
    let decoded = PublicParent::parse(buffer.as_bytes()).unwrap();
    (
        bytes,
        Observation {
            prefix: decoded.prefix,
            valid: decoded.child.valid,
        },
    )
}

#[cfg(not(miri))]
pub fn roundtrip_truly_private(prefix: u8, valid: bool, code: BigCode) -> (Vec<u8>, Observation) {
    let value = PublicPrivateParent {
        prefix,
        child: PrivateChild { valid, code },
    };
    let buffer = value.encode().unwrap();
    let bytes = buffer.as_bytes().to_vec();
    let decoded = PublicPrivateParent::parse(buffer.as_bytes()).unwrap();
    (
        bytes,
        Observation {
            prefix: decoded.prefix,
            valid: decoded.child.valid,
        },
    )
}

pub fn truly_private_error_facts(bytes: &[u8]) -> ErrorFacts {
    PublicPrivateParent::parse(bytes)
        .err()
        .map(facts)
        .expect("invalid private child")
}

#[cfg(not(miri))]
pub fn child_message_unknown_facts(bytes: &[u8]) -> ErrorFacts {
    let mut buffer = zs::make_buffer_for!(ChildMessage);
    buffer.as_bytes_mut().copy_from_slice(bytes);
    ChildMessage::parse(buffer.as_bytes())
        .err()
        .map(facts)
        .expect("unknown child tag")
}

pub fn child_message_layout() -> (usize, usize, usize, &'static str, &'static str) {
    let layout = ChildMessage::LAYOUT;
    let variants = layout.variants();
    (
        layout.size(),
        layout.align(),
        variants.len(),
        variants[0].name(),
        variants[1].name(),
    )
}

pub fn private_error_facts(bytes: &[u8]) -> ErrorFacts {
    PublicParent::parse(bytes)
        .err()
        .map(facts)
        .expect("invalid private child")
}

#[cfg(not(miri))]
pub fn roundtrip_borrowed<'a>(text: &'a str, marker: &'a [u8; 2]) -> (Vec<u8>, usize) {
    let value = BorrowingParent {
        child: BorrowedChild { text },
        marker,
    };
    let buffer = value.encode().unwrap();
    let decoded = BorrowingParent::parse(buffer.as_bytes()).unwrap();
    let base = buffer.as_bytes().as_ptr() as usize;
    let text_offset = decoded.child.text.as_ptr() as usize - base;
    (buffer.as_bytes().to_vec(), text_offset)
}

pub fn generic_layout<const N: usize>() -> (usize, usize, usize) {
    (
        GenericParent::<'static, N>::WIRE_SIZE,
        GenericParent::<'static, N>::LAYOUT.fields()[0].size(),
        GenericBytes::<'static, N>::LAYOUT.fields()[0].size(),
    )
}

#[cfg(not(miri))]
pub fn composition_roundtrip(message: ChildMessage) -> (Vec<u8>, u8, u16, u8) {
    let value = CompositionParent {
        direct: DirectChild {
            valid: true,
            value: 0x1234,
        },
        projected: TrailingProjection {
            child: DirectChild {
                valid: false,
                value: 0x5678,
            },
            sentinel: 0xa5,
        },
        tagged: message,
    };
    let buffer = value.encode().unwrap();
    let decoded = CompositionParent::parse(buffer.as_bytes()).unwrap();
    (
        buffer.as_bytes().to_vec(),
        u8::from(decoded.direct.valid),
        decoded.projected.child.value,
        decoded.projected.sentinel,
    )
}
