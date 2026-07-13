use core::mem::{align_of, size_of};

use widestring::{U16CStr, U16Str};
use zero_schema::{__private::*, IntegerRepr, LayoutError};

#[path = "support/counting_alloc.rs"]
mod counting_alloc;
use counting_alloc::allocations;

macro_rules! explicit_wire {
    ($ty:ty, $primitive:ty, $value:expr, $bytes:expr) => {{
        let wire = <$ty>::new($value as $primitive);
        assert_eq!(wire.get(), $value as $primitive);
        assert_eq!(wire.bytes().as_slice(), &$bytes);
        assert_eq!(size_of::<$ty>(), size_of::<$primitive>());
        assert_eq!(align_of::<$ty>(), align_of::<$primitive>());
    }};
}

#[test]
fn every_integer_and_float_wire_has_exact_bytes() {
    assert_eq!(U8::new(0xa5).get(), 0xa5);
    assert_eq!(I8::new(-2).get(), -2);
    assert_eq!(
        NativeU16::new(0x1234).get().to_ne_bytes(),
        0x1234u16.to_ne_bytes()
    );
    assert_eq!(
        NativeI16::new(-2).get().to_ne_bytes(),
        (-2i16).to_ne_bytes()
    );
    assert_eq!(
        NativeU32::new(0x12345678).get().to_ne_bytes(),
        0x12345678u32.to_ne_bytes()
    );
    assert_eq!(NativeI32::new(-2).get(), -2);
    assert_eq!(NativeU64::new(7).get(), 7);
    assert_eq!(NativeI64::new(-7).get(), -7);
    explicit_wire!(LittleU16, u16, 0x1234, [0x34, 0x12]);
    explicit_wire!(BigU16, u16, 0x1234, [0x12, 0x34]);
    explicit_wire!(LittleI16, i16, -2, [0xfe, 0xff]);
    explicit_wire!(BigI16, i16, -2, [0xff, 0xfe]);
    explicit_wire!(LittleU32, u32, 0x12345678, [0x78, 0x56, 0x34, 0x12]);
    explicit_wire!(BigU32, u32, 0x12345678, [0x12, 0x34, 0x56, 0x78]);
    explicit_wire!(LittleI32, i32, -2, [0xfe, 0xff, 0xff, 0xff]);
    explicit_wire!(BigI32, i32, -2, [0xff, 0xff, 0xff, 0xfe]);
    explicit_wire!(LittleU64, u64, 0x0102030405060708, [8, 7, 6, 5, 4, 3, 2, 1]);
    explicit_wire!(BigU64, u64, 0x0102030405060708, [1, 2, 3, 4, 5, 6, 7, 8]);
    explicit_wire!(
        LittleI64,
        i64,
        -2,
        [0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
    );
    explicit_wire!(
        BigI64,
        i64,
        -2,
        [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe]
    );
    for bits in [0, 0x8000_0000, 0x7fc0_1234] {
        let v = f32::from_bits(bits);
        assert_eq!(NativeF32::new(v).get().to_bits(), bits);
        assert_eq!(LittleF32::new(v).bytes(), &bits.to_le_bytes());
        assert_eq!(BigF32::new(v).bytes(), &bits.to_be_bytes());
    }
    for bits in [0, 0x8000_0000_0000_0000, 0x7ff8_0000_0000_1234] {
        let v = f64::from_bits(bits);
        assert_eq!(NativeF64::new(v).get().to_bits(), bits);
        assert_eq!(LittleF64::new(v).bytes(), &bits.to_le_bytes());
        assert_eq!(BigF64::new(v).bytes(), &bits.to_be_bytes());
    }
}

#[test]
fn bool_scalar_metadata_and_helper_layout() {
    assert_eq!(BoolWire::encode(false).raw(), 0);
    assert_eq!(BoolWire::encode(true).raw(), 1);
    let invalid = [2u8];
    assert_eq!(
        DecodeInput::<BoolWire>::from_exact(&invalid)
            .unwrap()
            .wire()
            .decode(),
        None
    );
    assert_eq!(<u8 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U8);
    assert_eq!(<u16 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U16);
    assert_eq!(<u32 as ScalarRepr>::INTEGER_REPR, IntegerRepr::U32);
    assert_eq!(FixedStrWire::<LittleU32, 1>::LEN_OFFSET, 0);
    assert_eq!(FixedStrWire::<LittleU32, 1>::DATA_OFFSET, 4);
    assert_eq!(size_of::<FixedStrWire<LittleU32, 1>>(), 8);
    assert_eq!(align_of::<FixedStrWire<LittleU32, 1>>(), 4);
    assert_eq!(FixedU16StrWire::<U8, 1>::LEN_OFFSET, 0);
    assert_eq!(FixedU16StrWire::<U8, 1>::DATA_OFFSET, 2);
    assert_eq!(size_of::<FixedU16StrWire<U8, 1>>(), 4);
    assert_eq!(align_of::<FixedU16StrWire<U8, 1>>(), 2);
    #[repr(align(4))]
    struct A([u8; 8]);
    let a = A([1, 0, 0, 0, b'x', 9, 9, 9]);
    let input = DecodeInput::<FixedStrWire<LittleU32, 1>>::from_exact(&a.0).unwrap();
    assert_eq!(input.wire().len_wire().get(), 1);
    assert_eq!(input.wire().data(), b"x");
    assert_eq!(&input.bytes()[5..], &[9, 9, 9]);
}

#[test]
fn string_boundaries_tails_and_exact_encoding() {
    assert_eq!(decode_str(&U8::new(0), b"", true), Ok(""));
    assert_eq!(decode_str(&U8::new(3), b"abc", true), Ok("abc"));
    assert_eq!(
        decode_str(&U8::new(4), b"abc", false),
        Err(CodecError::LengthOutOfBounds {
            length: 4,
            capacity: 3
        })
    );
    assert!(matches!(
        decode_str(&U8::new(1), &[0xff], false),
        Err(CodecError::InvalidUtf8(_))
    ));
    assert_eq!(
        decode_str(&U8::new(1), b"a\0x", true),
        Err(CodecError::NonZeroTail { offset: 2 })
    );
    assert_eq!(decode_c_str(b"a\0b\0", false).unwrap().to_bytes(), b"a");
    assert_eq!(decode_c_str(b"a\0\0", true).unwrap().to_bytes(), b"a");
    assert_eq!(
        decode_c_str(b"a\0b", true),
        Err(CodecError::NonZeroTail { offset: 2 })
    );
    assert_eq!(decode_c_str(b"abc", false), Err(CodecError::MissingNul));
    assert_eq!(
        decode_u16_str(&U8::new(0), &[], true).unwrap().as_slice(),
        &[]
    );
    assert_eq!(
        decode_u16_str(&U8::new(2), &[1, 0xd800], true)
            .unwrap()
            .as_slice(),
        &[1, 0xd800]
    );
    assert_eq!(
        decode_u16_str(&U8::new(3), &[1, 2], false),
        Err(CodecError::LengthOutOfBounds {
            length: 3,
            capacity: 2
        })
    );
    assert_eq!(
        decode_u16_str(&U8::new(1), &[1, 0, 7], true),
        Err(CodecError::NonZeroTail { offset: 2 })
    );
    assert_eq!(
        decode_u16_c_str(&[0xd800, 0, 7], false).unwrap().as_slice(),
        &[0xd800]
    );
    assert_eq!(
        decode_u16_c_str(&[1, 0, 7], true),
        Err(CodecError::NonZeroTail { offset: 2 })
    );
    assert_eq!(
        decode_u16_c_str(&[1, 2], false),
        Err(CodecError::MissingNul)
    );
    let c = c"ab";
    let wide = U16Str::from_slice(&[0x1234, 0x5678]);
    let wide_c = U16CStr::from_slice(&[0x1234, 0]).unwrap();
    assert_eq!(validate_str_encode("abc", 3), Ok(()));
    assert!(matches!(
        validate_str_encode("abc", 2),
        Err(CodecError::CapacityExceeded {
            length: 3,
            capacity: 2
        })
    ));
    assert!(matches!(
        validate_c_str_encode(c, 2),
        Err(CodecError::CapacityExceeded {
            length: 3,
            capacity: 2
        })
    ));
    assert!(matches!(
        validate_u16_str_encode(wide, 1),
        Err(CodecError::CapacityExceeded {
            length: 2,
            capacity: 1
        })
    ));
    assert!(matches!(
        validate_u16_c_str_encode(wide_c, 1),
        Err(CodecError::CapacityExceeded {
            length: 2,
            capacity: 1
        })
    ));
    let mut bytes = [0xa5; 12];
    {
        let mut root = Prezeroed::new(&mut bytes);
        encode_length::<BigU16>(2, &mut root).unwrap();
        let mut s = root.subrange(2, 2).unwrap();
        encode_str("ab", &mut s).unwrap();
        let mut w = root.subrange(4, 4).unwrap();
        encode_u16_str(wide, &mut w).unwrap();
        let mut f = root.subrange(8, 3).unwrap();
        encode_fixed_bytes(b"xyz", &mut f).unwrap();
    }
    assert_eq!(
        &bytes[..],
        [
            0, 2, b'a', b'b', 0x34, 0x12, 0x78, 0x56, b'x', b'y', b'z', 0
        ]
    );
}

#[repr(align(8))]
struct Aligned([u8; 24]);
#[test]
fn layout_precedence_and_checked_overflow() {
    let storage = Aligned([0; 24]);
    let mis = &storage.0[1..];
    assert_eq!(
        DecodeInput::<u64>::from_exact(&mis[..7]).err(),
        Some(LayoutError::IncorrectSize {
            expected: 8,
            actual: 7
        })
    );
    assert_eq!(
        DecodeInput::<u64>::from_prefix(&mis[..7]).err(),
        Some(LayoutError::InsufficientBytes {
            required: 8,
            actual: 7
        })
    );
    assert!(matches!(
        DecodeInput::<u64>::from_exact(&mis[..8]),
        Err(LayoutError::Misaligned { required: 8, .. })
    ));
    assert_eq!(
        DecodeInput::<u64>::from_exact(&storage.0[..9]).err(),
        Some(LayoutError::IncorrectSize {
            expected: 8,
            actual: 9
        })
    );
    let input = DecodeInput::<u64>::from_exact(&storage.0[..8]).unwrap();
    assert_eq!(
        input.subrange::<u64>(usize::MAX).err(),
        Some(LayoutError::OffsetOverflow)
    );
    let mut one = [9];
    let mut out = Prezeroed::new(&mut one);
    assert_eq!(
        out.write(usize::MAX, &[1, 2]),
        Err(LayoutError::OffsetOverflow)
    );
    assert_eq!(
        out.subrange(usize::MAX, 2).err(),
        Some(LayoutError::OffsetOverflow)
    );
}

#[test]
fn direct_codec_paths_allocate_zero() {
    let length = U8::new(3);
    let input = *b"abc\0";
    let c = c"abc";
    let wide = U16Str::from_slice(&[1, 2]);
    let wide_c = U16CStr::from_slice(&[1, 0]).unwrap();
    let fixed = *b"xyz";
    let mut output = [0u8; 16];
    let (_, allocation_count) = allocations(|| {
        assert_eq!(decode_str(&length, &input[..3], true).unwrap(), "abc");
        assert_eq!(decode_c_str(&input, true).unwrap(), c);
        assert_eq!(
            decode_u16_str(&U8::new(2), wide.as_slice(), true).unwrap(),
            wide
        );
        assert_eq!(
            decode_u16_c_str(wide_c.as_slice_with_nul(), true).unwrap(),
            wide_c
        );
        let mut root = Prezeroed::new(&mut output);
        encode_str("abc", &mut root.subrange(0, 3).unwrap()).unwrap();
        encode_c_str(c, &mut root.subrange(3, 4).unwrap()).unwrap();
        encode_u16_str(wide, &mut root.subrange(7, 4).unwrap()).unwrap();
        encode_fixed_bytes(&fixed, &mut root.subrange(11, 3).unwrap()).unwrap();
    });
    assert_eq!(allocation_count, 0);
}
