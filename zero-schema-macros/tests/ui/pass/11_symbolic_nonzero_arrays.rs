#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
    Two = 2,
}

#[zero(crate = zs)]
pub struct GenericArrays<const N: usize> {
    values: [Required; N],
    maybe_values: Option<[Required; N]>,
}

#[zero(crate = zs)]
pub struct NestedArrays<const N: usize> {
    maybe_values: Option<[Required; N]>,
}

#[zero(crate = zs)]
pub struct NestedOuter<const N: usize> {
    required: Required,
    nested: NestedArrays<N>,
}

fn exercise<const N: usize>() {
    let mut bytes = vec![0_u8; GenericArrays::<N>::SCHEMA_SIZE];
    let values = GenericArrays::<N>::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "values")
        .expect("array descriptor");
    for index in 0..N {
        bytes[values.offset() + index] = Required::One as u8;
    }

    {
        let mut root = GenericArrays::<N>::access_mut(&mut bytes).expect("N is nonzero");
        root.values_mut()
            .copy_from(&[Required::Two; N])
            .expect("plain symbolic array mutates");
        root.maybe_values_mut()
            .set(Some([Required::One; N]))
            .expect("optional symbolic array initializes");
    }
    let copied = GenericArrays::<N>::access(&bytes)
        .expect("initialized arrays prove")
        .copy_into();
    assert_eq!(copied.values, [Required::Two; N]);
    assert_eq!(copied.maybe_values, Some([Required::One; N]));
}

fn exercise_nested<const N: usize>() {
    let mut bytes = vec![0_u8; NestedOuter::<N>::SCHEMA_SIZE];
    let required = NestedOuter::<N>::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "required")
        .expect("required descriptor");
    bytes[required.offset()] = Required::One as u8;

    NestedOuter::<N>::access_mut(&mut bytes)
        .expect("nested nonzero schema proves")
        .nested_mut()
        .maybe_values_mut()
        .set(Some([Required::Two; N]))
        .expect("nested optional symbolic array initializes");
    let nested = NestedOuter::<N>::access(&bytes)
        .expect("nested initialized schema proves")
        .nested();
    assert_eq!(nested.maybe_values().unwrap().copy_into(), [Required::Two; N]);
}

fn main() {
    exercise::<1>();
    exercise::<2>();
    exercise_nested::<1>();
    exercise_nested::<2>();
}
