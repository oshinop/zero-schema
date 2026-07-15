use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
}

#[zero]
pub struct Child<const N: usize> {
    values: Option<[Required; N]>,
}

#[zero]
pub struct Outer<const N: usize> {
    required: Required,
    child: Child<N>,
}

fn main() {
    let mut bytes = [Required::One as u8];
    let _ = Outer::<0>::access(&bytes);
    let _ = Outer::<0>::access_mut(&mut bytes);
}
