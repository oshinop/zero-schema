#![deny(warnings)]

use core::ffi::CStr;

use zero_schema_macros::zero;

type U16CStr = zs::__private::U16CStr;
type U16Str = zs::__private::U16Str;

#[zero(crate = zs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pair {
    id: u8,
    on: bool,
}

#[zero(crate = zs)]
pub struct Strings<'a, const N: usize> {
    #[zero(capacity = 4)]
    names: [&'a U16CStr; N],
    #[zero(capacity = 3, len_type = u8)]
    wide: [&'a U16Str; 2],
    #[zero(capacity = 4, len_type = u16, endian = "big")]
    text: [&'a str; 2],
    #[zero(capacity = 4)]
    codes: [&'a CStr; 2],
}

#[zero(crate = zs)]
pub struct Nested<const R: usize, const C: usize> {
    cells: [[u16; C]; R],
    flags: [[bool; 2]; 2],
    pairs: [[Pair; 2]; R],
    cube: [[[u8; 2]; 3]; 2],
}

#[zero(crate = zs)]
pub struct NestedStrings<'a> {
    #[zero(capacity = 3, len_type = u8)]
    words: [[&'a str; 2]; 2],
}

fn exercise_strings<const N: usize>() {
    let mut storage = vec![0_u8; Strings::<N>::SCHEMA_SIZE];
    let mut root = Strings::<N>::access_mut(&mut storage).expect("zero bytes are empty strings");
    {
        let mut names = root.names_mut();
        names
            .set(N - 1, U16CStr::from_slice(&[0x41, 0]).expect("single terminator"))
            .expect("fits capacity");
        names
            .get_mut(0)
            .expect("first element")
            .set(U16CStr::from_slice(&[0x42, 0x43, 0]).expect("single terminator"))
            .expect("fits capacity");
    }
    root.text_mut().set(1, "abc").expect("fits capacity");
    root.codes_mut().copy_from(&[c"x", c"yz"]).expect("fits capacity");
    root.wide_mut()
        .get_mut(1)
        .expect("second element")
        .set(U16Str::from_slice(&[0x44]))
        .expect("fits capacity");
    let copied = root.copy_into();
    assert_eq!(copied.names[0].as_slice(), &[0x42, 0x43]);
    assert_eq!(copied.names[N - 1].as_slice().len(), if N == 1 { 2 } else { 1 });
    assert_eq!(copied.text, ["", "abc"]);
    assert_eq!(copied.codes, [c"x", c"yz"]);
    assert_eq!(copied.wide[1].as_slice(), &[0x44]);
}

fn exercise_nested<const R: usize, const C: usize>() {
    let mut storage = vec![0_u8; Nested::<R, C>::SCHEMA_SIZE];
    let mut root = Nested::<R, C>::access_mut(&mut storage).expect("zero bytes prove");
    root.cells_mut()
        .get_mut(R - 1)
        .expect("last row")
        .set(C - 1, 9)
        .expect("in range");
    root.flags_mut().set(0, [true, false]).expect("exact row");
    root.pairs_mut()
        .set(R - 1, [Pair { id: 1, on: true }, Pair { id: 2, on: false }])
        .expect("exact row");
    root.cube_mut()
        .get_mut(1)
        .expect("second plane")
        .get_mut(2)
        .expect("third row")
        .set(1, 7)
        .expect("in range");
    let copied = root.copy_into();
    assert_eq!(copied.cells[R - 1][C - 1], 9);
    assert_eq!(copied.flags, [[true, false], [false, false]]);
    assert_eq!(copied.pairs[R - 1][0].id, 1);
    assert_eq!(copied.cube[1][2], [0, 7]);
    let _: [[u16; C]; R] = copied.cells;
}

fn exercise_nested_strings() {
    let mut storage = zs::make_schema_buffer!(NestedStrings<'static>);
    let mut root = NestedStrings::access_mut(storage.as_bytes_mut()).expect("zero bytes prove");
    root.words_mut().set(1, ["ab", "c"]).expect("fits capacity");
    let error = root.words_mut().set(0, ["ab", "toolong"]).expect_err("capacity");
    assert!(
        error
            .to_string()
            .starts_with("NestedStrings.words[0][1]: "),
        "{error}"
    );
    assert_eq!(root.copy_into().words, [["", ""], ["ab", "c"]]);
    let view = NestedStrings::access(storage.as_bytes()).expect("stays valid");
    let row: zs::ArrayRef<'_, &str, 2, _> = view.words().get(1).expect("row");
    assert_eq!(row.get(0), Some("ab"));
}

fn main() {
    exercise_strings::<1>();
    exercise_strings::<2>();
    exercise_nested::<1, 1>();
    exercise_nested::<2, 3>();
    exercise_nested_strings();
}
