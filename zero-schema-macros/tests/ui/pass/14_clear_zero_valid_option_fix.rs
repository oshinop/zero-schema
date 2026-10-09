#![deny(warnings)]

use core::ffi::CStr;

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Idle = 1,
    Busy = 2,
}

#[zero(crate = zs)]
pub struct Pair {
    id: u8,
    on: bool,
}

// A bare `Mode` field would make `clear()` unavailable (see
// `fail/81_clear_zero_invalid_record.rs`). `Option<Mode>` is absent when all-zero,
// so this record is clearable, also for symbolic array lengths.
#[zero(crate = zs)]
pub struct Clearable<'a, const N: usize> {
    mode: Option<Mode>,
    pairs: [[Pair; 2]; N],
    #[zero(capacity = 4)]
    codes: [&'a CStr; 2],
    #[zero(capacity = 3, len_type = u8)]
    text: &'a str,
}

const _: () = assert!(Clearable::<2>::ZERO_IS_VALID);

fn exercise<const N: usize>() {
    let mut storage = vec![0_u8; Clearable::<N>::SCHEMA_SIZE];
    let mut root = Clearable::<N>::access_mut(&mut storage).expect("zero bytes prove");
    root.mode_mut().set(Some(Mode::Busy)).expect("present");
    root.text_mut().set("abc").expect("fits capacity");
    root.codes_mut().copy_from(&[c"x", c"yz"]).expect("fits capacity");
    root.pairs_mut()
        .set(N - 1, [Pair { id: 1, on: true }, Pair { id: 2, on: false }])
        .expect("exact row");

    // A string handle, an array element, a nested array row's element and the root.
    root.text_mut().clear();
    assert_eq!(root.text_mut().get(), "");
    root.codes_mut().get_mut(1).expect("second code").clear();
    root.pairs_mut()
        .get_mut(N - 1)
        .expect("last row")
        .get_mut(0)
        .expect("first pair")
        .clear();
    root.clear();

    let copied = root.copy_into();
    assert_eq!(copied.mode, None);
    assert_eq!(copied.text, "");
    assert_eq!(copied.codes, [c"", c""]);
    assert_eq!(copied.pairs[N - 1][1].id, 0);
    assert!(storage.iter().all(|byte| *byte == 0));
}

fn main() {
    exercise::<1>();
    exercise::<3>();
}
