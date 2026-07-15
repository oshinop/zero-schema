#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleTextChild<'a> {
    required: Required,
    #[zero(capacity = 4)]
    text: &'a str,
}

#[zero(crate = zs)]
pub struct OptionalRoot<'a> {
    maybe_text: Option<EligibleTextChild<'a>>,
}

fn main() {
    let mut bytes = [0_u8; OptionalRoot::<'static>::SCHEMA_SIZE];
    {
        let mut root = OptionalRoot::access_mut(&mut bytes).expect("all-zero optional is absent");
        let mut maybe_text = root.maybe_text_mut();
        assert!(maybe_text.get().is_none());
        maybe_text
            .set(Some(EligibleTextChild {
                required: Required::One,
                text: "hi",
            }))
            .expect("source-lifetime child initializes through OptionMut");
        assert_eq!(maybe_text.get().expect("present child").text(), "hi");
    }
    let copied = OptionalRoot::access(&bytes)
        .expect("initialized text child is valid")
        .copy_into();
    assert_eq!(copied.maybe_text.expect("present logical child").text, "hi");
}
