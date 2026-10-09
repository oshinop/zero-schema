//! Fixed arrays of bounded strings and fixed arrays of fixed arrays.
//!
//! The producer bytes below follow the C/C++ layout of `wchar_t names[N][CAP]`
//! style members: each string element occupies its own wire stride and a nested
//! array is the row-major multidimensional array. Every fixture byte outside the
//! declared representation is `0xEE` so ignored storage is observable.

use core::ffi::CStr;

use widestring::{U16CStr, U16Str};
use zero_schema::{
    ArrayElementKind, ErrorKind, ErrorPathSegment, FieldKind, LengthRepr, SchemaError,
    StringEncoding, error_path_string, zero,
};

const UNUSED: u8 = 0xee;

#[zero]
#[derive(Debug, PartialEq)]
pub struct Roster<'b> {
    pub count: u32,
    #[zero(capacity = 5)]
    pub names: [&'b U16CStr; 2],
    #[zero(capacity = 3, len_type = u8)]
    pub wide: [&'b U16Str; 2],
    #[zero(capacity = 6, len_type = u8)]
    pub tags: [&'b str; 2],
    #[zero(capacity = 4)]
    pub codes: [&'b CStr; 2],
    pub grid: [[u16; 3]; 2],
}

type RosterBuffer = zero_schema::schema_buffer!(Roster<'static>);

fn offset(name: &str) -> usize {
    Roster::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .unwrap_or_else(|| panic!("field {name}"))
        .offset()
}

fn put16(bytes: &mut [u8], offset: usize, units: &[u16]) {
    for (index, unit) in units.iter().enumerate() {
        let at = offset + index * 2;
        bytes[at..at + 2].copy_from_slice(&unit.to_ne_bytes());
    }
}

fn roster() -> RosterBuffer {
    let mut buffer = RosterBuffer::new();
    let bytes = buffer.as_bytes_mut();
    bytes.fill(UNUSED);
    bytes[offset("count")..offset("count") + 4].copy_from_slice(&7_u32.to_ne_bytes());
    let names = offset("names");
    put16(bytes, names, &[0x41, 0x42, 0]);
    put16(bytes, names + 10, &[0x43, 0x44, 0x45, 0]);
    let wide = offset("wide");
    bytes[wide] = 2;
    put16(bytes, wide + 2, &[0x51, 0x52]);
    bytes[wide + 8] = 1;
    put16(bytes, wide + 10, &[0x53]);
    let tags = offset("tags");
    bytes[tags] = 2;
    bytes[tags + 1..tags + 3].copy_from_slice(b"hi");
    bytes[tags + 7] = 3;
    bytes[tags + 8..tags + 11].copy_from_slice(b"yo!");
    let codes = offset("codes");
    bytes[codes..codes + 3].copy_from_slice(b"ab\0");
    bytes[codes + 4..codes + 8].copy_from_slice(b"cde\0");
    put16(bytes, offset("grid"), &[1, 2, 3, 4, 5, 6]);
    buffer
}

fn text(value: &U16CStr) -> String {
    char::decode_utf16(value.as_slice().iter().copied())
        .map(|unit| unit.unwrap_or('?'))
        .collect()
}

fn within(bytes: &[u8], pointer: *const u8, length: usize) -> bool {
    let start = bytes.as_ptr() as usize;
    let at = pointer as usize;
    at >= start && at + length <= start + bytes.len()
}

#[test]
fn string_and_nested_array_layout_is_row_major_c_layout_metadata() {
    // Strides are the element wire sizes; there is no extra inter-element padding.
    assert_eq!(offset("names"), 4);
    assert_eq!(offset("wide"), 24);
    assert_eq!(offset("tags"), 40);
    assert_eq!(offset("codes"), 54);
    assert_eq!(offset("grid"), 62);
    assert_eq!(Roster::SCHEMA_SIZE, 76);

    let fields = Roster::LAYOUT.fields();
    let by_name = |name: &str| fields.iter().find(|f| f.name() == name).unwrap();

    let FieldKind::Array(names) = by_name("names").kind() else {
        panic!("names is an array")
    };
    assert_eq!((names.length(), names.stride()), (2, 10));
    let ArrayElementKind::String(string) = names.element() else {
        panic!("names elements are strings")
    };
    assert_eq!(string.encoding(), StringEncoding::U16C);
    assert_eq!(
        (string.capacity(), string.length(), string.data_offset()),
        (5, None, 0)
    );
    assert_eq!(by_name("names").size(), 20);

    let FieldKind::Array(wide) = by_name("wide").kind() else {
        panic!("wide is an array")
    };
    assert_eq!((wide.length(), wide.stride()), (2, 8));
    let ArrayElementKind::String(string) = wide.element() else {
        panic!("wide elements are strings")
    };
    assert_eq!(string.encoding(), StringEncoding::U16);
    assert_eq!(string.data_offset(), 2);
    let length = string.length().expect("prefixed wide string");
    assert_eq!((length.repr(), length.offset()), (LengthRepr::U8, 0));

    let FieldKind::Array(tags) = by_name("tags").kind() else {
        panic!("tags is an array")
    };
    assert_eq!((tags.length(), tags.stride()), (2, 7));
    let ArrayElementKind::String(string) = tags.element() else {
        panic!("tags elements are strings")
    };
    assert_eq!(
        (string.encoding(), string.data_offset()),
        (StringEncoding::Utf8, 1)
    );

    let FieldKind::Array(codes) = by_name("codes").kind() else {
        panic!("codes is an array")
    };
    assert_eq!((codes.length(), codes.stride()), (2, 4));
    assert!(matches!(
        codes.element(),
        ArrayElementKind::String(s) if s.encoding() == StringEncoding::CBytes
    ));

    let FieldKind::Array(grid) = by_name("grid").kind() else {
        panic!("grid is an array")
    };
    assert_eq!((grid.length(), grid.stride()), (2, 6));
    let ArrayElementKind::Array(row) = grid.element() else {
        panic!("grid elements are rows")
    };
    assert_eq!((row.length(), row.stride()), (3, 2));
    assert!(matches!(row.element(), ArrayElementKind::Primitive { .. }));
    assert_eq!(by_name("grid").size(), 12);
}

/// `wchar_t names[4][49]`, `wchar_t nick[2][41]`, and a `uint16_t raw[4][49]`.
#[zero]
#[derive(Debug, PartialEq)]
pub struct Wide<'b> {
    #[zero(capacity = 49)]
    pub names: [&'b U16CStr; 4],
    #[zero(capacity = 41)]
    pub nick: [&'b U16CStr; 2],
    pub raw: [[u16; 49]; 4],
}

#[test]
fn real_world_wchar_arrays_have_the_cxx_multidimensional_layout() {
    let fields = Wide::LAYOUT.fields();
    assert_eq!(
        fields
            .iter()
            .map(|f| (f.offset(), f.size()))
            .collect::<Vec<_>>(),
        vec![
            (0, 4 * 49 * 2),
            (4 * 49 * 2, 2 * 41 * 2),
            (4 * 49 * 2 + 2 * 41 * 2, 4 * 49 * 2)
        ]
    );
    assert_eq!(Wide::SCHEMA_SIZE, 4 * 98 + 2 * 82 + 4 * 98);
    assert_eq!(Wide::SCHEMA_ALIGN, 2);

    let mut buffer = zero_schema::make_schema_buffer!(Wide);
    let bytes = buffer.as_bytes_mut();
    bytes.fill(0);
    for (row, text) in ["Ann", "Bo", "", "Dee"].iter().enumerate() {
        for (column, unit) in text.encode_utf16().enumerate() {
            let at = (row * 49 + column) * 2;
            bytes[at..at + 2].copy_from_slice(&unit.to_ne_bytes());
        }
    }
    let raw = 4 * 98 + 2 * 82;
    for row in 0..4_usize {
        for column in 0..49_usize {
            let value = (row * 100 + column) as u16;
            let at = raw + (row * 49 + column) * 2;
            bytes[at..at + 2].copy_from_slice(&value.to_ne_bytes());
        }
    }
    let view = Wide::access(buffer.as_bytes()).unwrap();
    let names = view.names();
    assert_eq!(text(names.get(0).unwrap()), "Ann");
    assert_eq!(text(names.get(2).unwrap()), "");
    assert_eq!(text(names.get(3).unwrap()), "Dee");
    assert!(names.get(4).is_none());
    assert_eq!(view.nick().get(1).unwrap().len(), 0);
    let raw_rows = view.raw();
    assert_eq!(raw_rows.get(3).unwrap().get(48), Some(348));
    assert_eq!(raw_rows.get(1).unwrap().get(0), Some(100));
    assert!(raw_rows.get(4).is_none());
    assert!(raw_rows.get(0).unwrap().get(49).is_none());
}

#[test]
fn string_arrays_read_zero_copy_and_materialize_the_borrowed_array() {
    let buffer = roster();
    let bytes = buffer.as_bytes();
    let view = Roster::access(bytes).unwrap();

    let names = view.names();
    let first = names.get(0).unwrap();
    assert_eq!(text(first), "AB");
    assert!(within(bytes, first.as_ptr().cast(), 6));
    assert_eq!(text(names.get(1).unwrap()), "CDE");
    assert!(names.get(2).is_none());
    assert_eq!(names.iter().map(text).collect::<Vec<_>>(), ["AB", "CDE"]);
    let materialized: [&U16CStr; 2] = names.copy_into();
    assert_eq!(materialized[1].as_slice(), &[0x43, 0x44, 0x45]);

    assert_eq!(view.wide().get(0).unwrap().as_slice(), &[0x51, 0x52]);
    assert_eq!(view.wide().get(1).unwrap().as_slice(), &[0x53]);
    let tags = view.tags();
    assert_eq!(tags.get(0), Some("hi"));
    assert_eq!(tags.get(1), Some("yo!"));
    assert!(within(bytes, tags.get(0).unwrap().as_ptr(), 2));
    assert_eq!(view.codes().get(0), Some(c"ab"));
    assert_eq!(view.codes().get(1), Some(c"cde"));

    let grid = view.grid();
    assert_eq!(grid.get(0).unwrap().copy_into(), [1, 2, 3]);
    assert_eq!(grid.get(1).unwrap().iter().collect::<Vec<_>>(), [4, 5, 6]);
    assert_eq!(grid.get(1).unwrap().get(2), Some(6));
    assert!(grid.get(2).is_none());
    assert_eq!(grid.iter().count(), 2);

    let logical = view.copy_into();
    assert_eq!(logical.count, 7);
    assert_eq!(logical.names[0].as_slice(), &[0x41, 0x42]);
    assert_eq!(logical.names[1].as_slice(), &[0x43, 0x44, 0x45]);
    assert_eq!(logical.wide[1].as_slice(), &[0x53]);
    assert_eq!(logical.tags, ["hi", "yo!"]);
    assert_eq!(logical.codes, [c"ab", c"cde"]);
    assert_eq!(logical.grid, [[1, 2, 3], [4, 5, 6]]);
}

#[test]
fn string_array_element_handles_preserve_unused_capacity_and_write_in_place() {
    let mut buffer = roster();
    let names_at = offset("names");
    let wide_at = offset("wide");
    let tags_at = offset("tags");
    let codes_at = offset("codes");
    {
        let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
        {
            let mut names = view.names_mut();
            // Reads through the mutable array stay zero-copy.
            assert_eq!(text(names.get(0).unwrap()), "AB");
            names
                .get_mut(1)
                .unwrap()
                .set(U16CStr::from_slice(&[0x5a, 0]).unwrap())
                .unwrap();
            assert_eq!(text(names.get(1).unwrap()), "Z");
            assert!(names.get_mut(2).is_none());
            names
                .set(
                    0,
                    U16CStr::from_slice(&[0x31, 0x32, 0x33, 0x34, 0]).unwrap(),
                )
                .unwrap();
        }
        view.wide_mut()
            .get_mut(0)
            .unwrap()
            .set(U16Str::from_slice(&[0x61]))
            .unwrap();
        view.tags_mut().get_mut(1).unwrap().set("é").unwrap();
        view.tags_mut().set(0, "").unwrap();
        view.codes_mut().get_mut(0).unwrap().set(c"xyz").unwrap();
    }
    let bytes = buffer.as_bytes();
    // names[0] now fills its capacity; names[1] keeps stale units after "Z\0".
    assert_eq!(
        &bytes[names_at..names_at + 10],
        &native(&[0x31, 0x32, 0x33, 0x34, 0])
    );
    assert_eq!(
        &bytes[names_at + 10..names_at + 20],
        native(&[0x5a, 0, 0x45, 0, 0xeeee]).as_slice()
    );
    // Only the active prefix of wide[0] and tags[1] changed.
    assert_eq!(bytes[wide_at], 1);
    assert_eq!(&bytes[wide_at + 2..wide_at + 4], &native(&[0x61]));
    assert_eq!(&bytes[wide_at + 4..wide_at + 6], &native(&[0x52]));
    assert_eq!(bytes[tags_at], 0);
    assert_eq!(&bytes[tags_at + 1..tags_at + 3], b"hi");
    assert_eq!(bytes[tags_at + 7], 2);
    assert_eq!(&bytes[tags_at + 8..tags_at + 10], "é".as_bytes());
    assert_eq!(bytes[tags_at + 10], b'!');
    assert_eq!(&bytes[codes_at..codes_at + 4], b"xyz\0");
    assert_eq!(&bytes[codes_at + 4..codes_at + 8], b"cde\0");
    assert!(Roster::access(bytes).is_ok());
}

#[test]
fn string_array_element_handles_report_the_declared_capacity() {
    // Every element handle reports the field's declared `capacity` in the units
    // of a string field handle: bytes for `str`/`CStr`, `u16` units for the wide
    // forms, terminator counted for the C forms, length prefix excluded.
    let mut buffer = roster();
    let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
    assert_eq!(
        [
            view.names_mut().get_mut(1).unwrap().capacity(),
            view.wide_mut().get_mut(0).unwrap().capacity(),
            view.tags_mut().get_mut(1).unwrap().capacity(),
            view.codes_mut().get_mut(0).unwrap().capacity(),
        ],
        [5, 3, 6, 4]
    );
    // The capacity does not follow the active value.
    view.tags_mut().set(1, "").unwrap();
    assert_eq!(view.tags_mut().get_mut(1).unwrap().capacity(), 6);

    // The innermost handle of a nested string array reports the same capacity.
    let mut buffer = deep();
    let mut view = Deep::access_mut(buffer.as_bytes_mut()).unwrap();
    let mut words = view.words_mut();
    let mut row = words.get_mut(1).unwrap();
    assert_eq!(row.get_mut(1).unwrap().capacity(), 3);
}

fn native(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_ne_bytes()).collect()
}

#[test]
fn string_array_failures_are_indexed_and_leave_every_byte_unchanged() {
    // Eager access reports the failing element's index path.
    let mut buffer = roster();
    let names_at = offset("names");
    put16(
        buffer.as_bytes_mut(),
        names_at + 10,
        &[0x43, 0x44, 0x45, 0x46, 0x47],
    );
    let error = Roster::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingNul);
    assert_eq!(error_path_string(&error), "Roster.names[1]");
    assert_eq!(error.segment(), Some(ErrorPathSegment::Field("names")));
    assert_eq!(
        error.child().unwrap().segment(),
        Some(ErrorPathSegment::Index(1))
    );

    let mut buffer = roster();
    buffer.as_bytes_mut()[offset("tags") + 7 + 1] = 0xff;
    let error = Roster::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidUtf8);
    assert_eq!(error_path_string(&error), "Roster.tags[1]");

    let mut buffer = roster();
    buffer.as_bytes_mut()[offset("wide") + 8] = 9;
    let error = Roster::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LengthOutOfBounds);
    assert_eq!(error_path_string(&error), "Roster.wide[1]");

    let mut buffer = roster();
    buffer.as_bytes_mut()[offset("codes")..offset("codes") + 4].copy_from_slice(b"abcd");
    let error = Roster::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingNul);
    assert_eq!(error_path_string(&error), "Roster.codes[0]");

    // Bounded element mutation failures are indexed and atomic.
    let mut buffer = roster();
    let before = buffer.as_bytes().to_vec();
    let long = U16CStr::from_slice(&[1, 2, 3, 4, 5, 0]).unwrap();
    let short = U16CStr::from_slice(&[9, 0]).unwrap();
    {
        let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
        let error = view.names_mut().set(1, long).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Roster.names[1]");

        let error = view.names_mut().copy_from(&[short, long]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Roster.names[1]");

        let error = view.names_mut().copy_from(&[short]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArrayLengthMismatch);
        assert_eq!(error.segment(), Some(ErrorPathSegment::Field("names")));

        let error = view.names_mut().set(2, short).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArrayIndexOutOfBounds);
        assert_eq!(error_path_string(&error), "Roster.names[2]");

        let error = view.tags_mut().copy_from(&["ok", "toolong"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Roster.tags[1]");

        // Narrow length prefixes are `u8`, so capacity above 255 is not reachable
        // here; a capacity failure on the per-element handle is field-level.
        let mut tags = view.tags_mut();
        let mut handle = tags.get_mut(0).unwrap();
        let error = handle.set("toolong").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error.segment(), Some(ErrorPathSegment::Field("tags")));

        let error = view
            .codes_mut()
            .copy_from(&[c"ok", c"toolong"])
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Roster.codes[1]");
    }
    assert_eq!(buffer.as_bytes(), before.as_slice());
}

#[test]
fn string_array_copy_from_patch_and_whole_value_movement_are_exact() {
    let mut buffer = roster();
    {
        let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
        view.names_mut()
            .copy_from(&[
                U16CStr::from_slice(&[0x61, 0]).unwrap(),
                U16CStr::from_slice(&[0x62, 0x63, 0]).unwrap(),
            ])
            .unwrap();
        view.codes_mut().copy_from(&[c"q", c"rs"]).unwrap();
        view.copy_from(&RosterPatch {
            tags: Some(["a", "bc"]),
            grid: Some([[9, 8, 7], [6, 5, 4]]),
            ..Default::default()
        })
        .unwrap();
        let again = view.copy_into();
        assert_eq!(again.names[0].as_slice(), &[0x61]);
        assert_eq!(again.names[1].as_slice(), &[0x62, 0x63]);
        assert_eq!(again.tags, ["a", "bc"]);
        assert_eq!(again.codes, [c"q", c"rs"]);
        assert_eq!(again.grid, [[9, 8, 7], [6, 5, 4]]);
        // Patch entries the caller omitted keep their prior value.
        assert_eq!(again.wide[0].as_slice(), &[0x51, 0x52]);
        assert_eq!(again.count, 7);
    }

    // A patch whose later element is rejected leaves every byte unchanged.
    let before = buffer.as_bytes().to_vec();
    let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
    let error = view
        .copy_from(&RosterPatch {
            count: Some(99),
            tags: Some(["fine", "toolong"]),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(error_path_string(&error), "Roster.tags[1]");
    assert_eq!(buffer.as_bytes(), before.as_slice());

    // The logical value moves back in exactly.
    let logical = Roster::access(&before).unwrap().copy_into();
    let mut view = Roster::access_mut(buffer.as_bytes_mut()).unwrap();
    view.copy_from(&RosterPatch::from(logical)).unwrap();
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pair {
    pub id: u8,
    pub on: bool,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Label<'b> {
    #[zero(capacity = 3)]
    pub text: &'b CStr,
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Deep<'b> {
    pub flags: [[bool; 2]; 2],
    pub pairs: [[Pair; 2]; 3],
    #[zero(capacity = 3, len_type = u8)]
    pub words: [[&'b str; 2]; 2],
    pub cube: [[[u8; 2]; 3]; 2],
    pub labels: [[Label<'b>; 2]; 2],
    pub tail: u8,
}

type DeepBuffer = zero_schema::schema_buffer!(Deep<'static>);

fn deep_offset(name: &str) -> usize {
    Deep::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .unwrap_or_else(|| panic!("field {name}"))
        .offset()
}

fn deep() -> DeepBuffer {
    let mut buffer = DeepBuffer::new();
    let bytes = buffer.as_bytes_mut();
    bytes.fill(UNUSED);
    let flags = deep_offset("flags");
    bytes[flags..flags + 4].copy_from_slice(&[1, 0, 0, 1]);
    let pairs = deep_offset("pairs");
    for index in 0..6_usize {
        bytes[pairs + index * 2] = index as u8 + 10;
        bytes[pairs + index * 2 + 1] = (index % 2) as u8;
    }
    let words = deep_offset("words");
    for (index, text) in ["a", "bc", "def", ""].iter().enumerate() {
        let at = words + index * 4;
        bytes[at] = text.len() as u8;
        bytes[at + 1..at + 1 + text.len()].copy_from_slice(text.as_bytes());
    }
    let cube = deep_offset("cube");
    for index in 0..12_usize {
        bytes[cube + index] = index as u8 + 1;
    }
    let labels = deep_offset("labels");
    for (index, text) in [
        [b'w', 0, UNUSED],
        [b'x', b'y', 0],
        [b'z', 0, UNUSED],
        [0, UNUSED, UNUSED],
    ]
    .iter()
    .enumerate()
    {
        let at = labels + index * 3;
        bytes[at..at + 3].copy_from_slice(text);
    }
    bytes[deep_offset("tail")] = 0x7e;
    buffer
}

#[test]
fn nested_arrays_read_through_row_views_and_materialize_row_major() {
    // Strides follow the C++ multidimensional array: rows are contiguous.
    assert_eq!(Deep::LAYOUT.fields().len(), 6);
    let fields = Deep::LAYOUT.fields();
    let FieldKind::Array(cube) = fields[3].kind() else {
        panic!("cube is an array")
    };
    assert_eq!((cube.length(), cube.stride()), (2, 6));
    let ArrayElementKind::Array(plane) = cube.element() else {
        panic!("cube elements are planes")
    };
    assert_eq!((plane.length(), plane.stride()), (3, 2));
    let ArrayElementKind::Array(row) = plane.element() else {
        panic!("planes contain rows")
    };
    assert_eq!((row.length(), row.stride()), (2, 1));
    assert!(matches!(row.element(), ArrayElementKind::Primitive { .. }));

    let buffer = deep();
    let view = Deep::access(buffer.as_bytes()).unwrap();
    assert_eq!(view.flags().get(0).unwrap().copy_into(), [true, false]);
    assert_eq!(view.flags().get(1).unwrap().get(1), Some(true));
    let pairs = view.pairs();
    let pair = pairs.get(2).unwrap().get(1).unwrap();
    assert_eq!((pair.id(), pair.on()), (15, true));
    assert_eq!(pairs.get(0).unwrap().get(0).unwrap().id(), 10);
    assert!(pairs.get(3).is_none());
    assert_eq!(view.words().get(1).unwrap().get(0), Some("def"));
    assert_eq!(view.words().get(1).unwrap().get(1), Some(""));
    assert_eq!(
        view.words()
            .iter()
            .map(|row| row.copy_into())
            .collect::<Vec<_>>(),
        [["a", "bc"], ["def", ""]]
    );
    assert_eq!(view.cube().get(1).unwrap().get(2).unwrap().get(1), Some(12));
    assert_eq!(
        view.cube().get(0).unwrap().get(1).unwrap().copy_into(),
        [3, 4]
    );
    assert_eq!(view.labels().get(0).unwrap().get(1).unwrap().text(), c"xy");

    let logical = view.copy_into();
    assert_eq!(logical.flags, [[true, false], [false, true]]);
    assert_eq!(logical.pairs[1][0], Pair { id: 12, on: false });
    assert_eq!(logical.words, [["a", "bc"], ["def", ""]]);
    assert_eq!(
        logical.cube,
        [[[1, 2], [3, 4], [5, 6]], [[7, 8], [9, 10], [11, 12]]]
    );
    assert_eq!(logical.labels[1][0].text, c"z");
    assert_eq!(logical.tail, 0x7e);
}

#[test]
fn nested_array_access_errors_name_every_enclosing_index() {
    let flags = deep_offset("flags");
    let mut buffer = deep();
    buffer.as_bytes_mut()[flags + 2] = 2;
    let error = Deep::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidBool);
    assert_eq!(error_path_string(&error), "Deep.flags[1][0]");

    let mut buffer = deep();
    buffer.as_bytes_mut()[deep_offset("pairs") + 5 * 2 + 1] = 7;
    let error = Deep::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidBool);
    assert_eq!(error_path_string(&error), "Deep.pairs[2][1].on");

    let mut buffer = deep();
    // words[1][0] declares three active bytes of invalid UTF-8.
    let at = deep_offset("words") + 2 * 4;
    buffer.as_bytes_mut()[at + 1] = 0xff;
    let error = Deep::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidUtf8);
    assert_eq!(error_path_string(&error), "Deep.words[1][0]");
    let chain = {
        let mut segments = Vec::new();
        let mut current: &dyn SchemaError = &error;
        loop {
            segments.extend(current.segment());
            match current.child() {
                Some(child) => current = child,
                None => break,
            }
        }
        segments
    };
    assert_eq!(
        chain,
        [
            ErrorPathSegment::Field("words"),
            ErrorPathSegment::Index(1),
            ErrorPathSegment::Index(0)
        ]
    );

    let mut buffer = deep();
    buffer.as_bytes_mut()[deep_offset("labels") + 3 * 3..deep_offset("labels") + 3 * 3 + 3]
        .copy_from_slice(b"abc");
    let error = Deep::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingNul);
    assert_eq!(error_path_string(&error), "Deep.labels[1][1].text");
}

#[test]
fn nested_array_mutation_uses_rows_values_and_exact_transfer() {
    let mut buffer = deep();
    {
        let mut view = Deep::access_mut(buffer.as_bytes_mut()).unwrap();
        {
            let mut flags = view.flags_mut();
            flags
                .get_mut(1)
                .unwrap()
                .get_mut(0)
                .unwrap()
                .set(true)
                .unwrap();
            assert_eq!(flags.get(1).unwrap().copy_into(), [true, true]);
            flags.set(0, [false, true]).unwrap();
            assert_eq!(
                flags.copy_into().map(|row| row.copy_into()),
                [[false, true], [true, true]]
            );
        }
        {
            let mut words = view.words_mut();
            words
                .get_mut(0)
                .unwrap()
                .get_mut(1)
                .unwrap()
                .set("zzz")
                .unwrap();
            words.get_mut(1).unwrap().set(0, "q").unwrap();
            assert_eq!(words.get(0).unwrap().get(1), Some("zzz"));
            assert_eq!(words.get(1).unwrap().get(0), Some("q"));
            words.set(1, ["m", "n"]).unwrap();
        }
        view.cube_mut()
            .get_mut(1)
            .unwrap()
            .get_mut(2)
            .unwrap()
            .set(1, 99)
            .unwrap();
        view.pairs_mut()
            .get_mut(0)
            .unwrap()
            .get_mut(1)
            .unwrap()
            .id_mut()
            .set(200)
            .unwrap();
        view.pairs_mut()
            .set(2, [Pair { id: 1, on: true }, Pair { id: 2, on: false }])
            .unwrap();
        view.labels_mut()
            .copy_from(&[
                [Label { text: c"a" }, Label { text: c"b" }],
                [Label { text: c"cd" }, Label { text: c"" }],
            ])
            .unwrap();
    }
    let view = Deep::access(buffer.as_bytes()).unwrap();
    let logical = view.copy_into();
    assert_eq!(logical.flags, [[false, true], [true, true]]);
    assert_eq!(logical.words, [["a", "zzz"], ["m", "n"]]);
    assert_eq!(logical.cube[1][2], [11, 99]);
    assert_eq!(logical.pairs[0][1].id, 200);
    assert_eq!(
        logical.pairs[2],
        [Pair { id: 1, on: true }, Pair { id: 2, on: false }]
    );
    assert_eq!(logical.labels[1][0].text, c"cd");
    assert_eq!(logical.tail, 0x7e);
    // Only declared bytes changed: the sentinel after `labels[0][0]`'s NUL stays.
    assert_eq!(buffer.as_bytes()[deep_offset("labels") + 2], UNUSED);

    // Exact full-array transfer, then a whole-value patch.
    let mut view = Deep::access_mut(buffer.as_bytes_mut()).unwrap();
    view.cube_mut()
        .copy_from(&[[[0, 1], [2, 3], [4, 5]], [[6, 7], [8, 9], [10, 11]]])
        .unwrap();
    view.copy_from(&DeepPatch {
        words: Some([["x", "y"], ["z", "w"]]),
        ..Default::default()
    })
    .unwrap();
    let logical = view.copy_into();
    assert_eq!(
        logical.cube,
        [[[0, 1], [2, 3], [4, 5]], [[6, 7], [8, 9], [10, 11]]]
    );
    assert_eq!(logical.words, [["x", "y"], ["z", "w"]]);
}

#[test]
fn nested_array_mutation_failures_are_indexed_by_full_path_and_atomic() {
    let mut buffer = deep();
    let before = buffer.as_bytes().to_vec();
    {
        let mut view = Deep::access_mut(buffer.as_bytes_mut()).unwrap();

        // Whole-array operations report every enclosing index.
        let error = view.words_mut().set(1, ["ok", "toolong"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Deep.words[1][1]");

        let error = view
            .words_mut()
            .copy_from(&[["a", "b"], ["c", "toolong"]])
            .unwrap_err();
        assert_eq!(error_path_string(&error), "Deep.words[1][1]");

        // A row handle knows where it sits in the enclosing array.
        {
            let mut words = view.words_mut();
            let mut row = words.get_mut(1).unwrap();
            let error = row.set(0, "toolong").unwrap_err();
            assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
            assert_eq!(error_path_string(&error), "Deep.words[1][0]");
            let error = row.copy_from(&["fine", "toolong"]).unwrap_err();
            assert_eq!(error_path_string(&error), "Deep.words[1][0]");
            // An out-of-range index reports the offending index of the handle
            // that rejected it, and a short slice is a field-level length error.
            let error = row.set(2, "x").unwrap_err();
            assert_eq!(error.kind(), ErrorKind::ArrayIndexOutOfBounds);
            assert_eq!(error_path_string(&error), "Deep.words[2]");
            let error = row.copy_from(&["only"]).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::ArrayLengthMismatch);
        }
        let error = view.words_mut().set(2, ["a", "b"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArrayIndexOutOfBounds);
        assert_eq!(error_path_string(&error), "Deep.words[2]");

        // The per-element string handle owns only its string.
        {
            let mut words = view.words_mut();
            let mut row = words.get_mut(0).unwrap();
            let mut element = row.get_mut(1).unwrap();
            let error = element.set("toolong").unwrap_err();
            assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
            assert_eq!(error.segment(), Some(ErrorPathSegment::Field("words")));
        }

        // Schema leaves: a failing logical child is indexed through every level.
        let error = view
            .labels_mut()
            .copy_from(&[
                [Label { text: c"a" }, Label { text: c"b" }],
                [Label { text: c"c" }, Label { text: c"toolong" }],
            ])
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Deep.labels[1][1]");

        let error = view
            .copy_from(&DeepPatch {
                tail: Some(1),
                labels: Some([
                    [Label { text: c"a" }, Label { text: c"toolong" }],
                    [Label { text: c"c" }, Label { text: c"d" }],
                ]),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
        assert_eq!(error_path_string(&error), "Deep.labels[0][1]");

        let error = view
            .cube_mut()
            .copy_from(&[[[0, 1], [2, 3], [4, 5]]])
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArrayLengthMismatch);
    }
    assert_eq!(buffer.as_bytes(), before.as_slice());
}

/// A depth-three string array exercises the generic index chain.
#[zero]
#[derive(Debug, PartialEq)]
pub struct Cube<'b> {
    #[zero(capacity = 2, len_type = u8)]
    pub cells: [[[&'b str; 2]; 3]; 2],
}

#[test]
fn nested_array_depth_three_error_paths_decode_all_levels() {
    let mut buffer = zero_schema::make_schema_buffer!(Cube);
    let bytes = buffer.as_bytes_mut();
    bytes.fill(0);
    // The wire is 12 StrWire<U8, 2> elements of 3 bytes: flat index 9 is [1][1][1].
    bytes[9 * 3] = 3;
    let error = Cube::access(buffer.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LengthOutOfBounds);
    assert_eq!(error_path_string(&error), "Cube.cells[1][1][1]");
    buffer.as_bytes_mut()[9 * 3] = 0;

    let mut view = Cube::access_mut(buffer.as_bytes_mut()).unwrap();
    let mut cells = view.cells_mut();
    let mut plane = cells.get_mut(1).unwrap();
    let mut row = plane.get_mut(2).unwrap();
    let error = row.set(1, "abc").unwrap_err();
    assert_eq!(error_path_string(&error), "Cube.cells[1][2][1]");
    row.set(1, "ab").unwrap();
    assert_eq!(row.get(1), Some("ab"));
    assert_eq!(cells.get(1).unwrap().get(2).unwrap().get(1), Some("ab"));
}

pub const COLUMNS: usize = 2;
pub const ROWS: usize = 3;

#[zero]
#[derive(Debug, PartialEq)]
pub struct Generic<const R: usize, const C: usize> {
    pub cells: [[u16; C]; R],
    pub fixed: [[u8; COLUMNS]; ROWS],
}

#[test]
fn nested_arrays_compose_with_const_generic_and_path_lengths() {
    assert_eq!(Generic::<2, 3>::SCHEMA_SIZE, 2 * 3 * 2 + 3 * 2);
    let mut buffer = zero_schema::make_schema_buffer!(Generic<2, 3>);
    let bytes = buffer.as_bytes_mut();
    bytes.fill(0);
    let mut view = Generic::<2, 3>::access_mut(bytes).unwrap();
    view.cells_mut().get_mut(1).unwrap().set(2, 7).unwrap();
    view.fixed_mut().set(2, [4, 5]).unwrap();
    let logical = view.copy_into();
    assert_eq!(logical.cells, [[0, 0, 0], [0, 0, 7]]);
    assert_eq!(logical.fixed, [[0, 0], [0, 0], [4, 5]]);
    let fields = Generic::<2, 3>::LAYOUT.fields();
    let FieldKind::Array(cells) = fields[0].kind() else {
        panic!("cells is an array")
    };
    assert_eq!((cells.length(), cells.stride()), (2, 6));
    let ArrayElementKind::Array(row) = cells.element() else {
        panic!("rows")
    };
    assert_eq!((row.length(), row.stride()), (3, 2));
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Holder<'b> {
    #[zero(capacity = 3, len_type = u8)]
    pub words: [&'b str; 2],
    pub grid: [[u8; 2]; 2],
}

#[zero]
#[derive(Debug, PartialEq)]
pub struct Parent<'b> {
    pub one: Holder<'b>,
    pub many: [Holder<'b>; 2],
}

#[test]
fn string_and_nested_arrays_compose_inside_child_records_and_record_arrays() {
    let mut buffer = zero_schema::make_schema_buffer!(Parent<'static>);
    let mut view = Parent::access_mut(buffer.as_bytes_mut()).unwrap();
    view.one_mut().words_mut().set(1, "ab").unwrap();
    view.one_mut()
        .grid_mut()
        .get_mut(1)
        .unwrap()
        .set(0, 5)
        .unwrap();
    view.many_mut()
        .set(
            1,
            Holder {
                words: ["x", "yz"],
                grid: [[1, 2], [3, 4]],
            },
        )
        .unwrap();
    let logical = view.copy_into();
    assert_eq!(logical.one.words, ["", "ab"]);
    assert_eq!(logical.one.grid, [[0, 0], [5, 0]]);
    assert_eq!(logical.many[1].words, ["x", "yz"]);
    assert_eq!(logical.many[1].grid, [[1, 2], [3, 4]]);
    assert_eq!(
        view.many().get(1).unwrap().grid().get(1).unwrap().get(1),
        Some(4)
    );

    // A whole-child patch and a whole-record move both go through the same
    // two-pass preflight; one rejected nested string rejects everything.
    let before = buffer.as_bytes().to_vec();
    let mut view = Parent::access_mut(buffer.as_bytes_mut()).unwrap();
    let bad = Holder {
        words: ["ok", "toolong"],
        grid: [[9, 9], [9, 9]],
    };
    let error = view.many_mut().set(0, bad).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(error_path_string(&error), "Parent.many[0]");
    let mut logical = Parent::access(&before).unwrap().copy_into();
    logical.many[0].words = ["ok", "toolong"];
    let error = view.copy_from(&ParentPatch::from(logical)).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::CapacityExceeded);
    assert_eq!(buffer.as_bytes(), before.as_slice());

    let mut view = Parent::access_mut(buffer.as_bytes_mut()).unwrap();
    view.copy_from(&ParentPatch {
        one: Some(HolderPatch {
            words: Some(["p", "q"]),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let logical = view.copy_into();
    assert_eq!(logical.one.words, ["p", "q"]);
    assert_eq!(logical.one.grid, [[0, 0], [5, 0]]);
}
