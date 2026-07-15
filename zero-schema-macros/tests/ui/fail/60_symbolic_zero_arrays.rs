use zero_schema_macros::zero;

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Required {
    One = 1,
}

#[zero]
pub struct Plain<const N: usize> {
    required: Required,
    values: [Required; N],
}

#[zero]
pub struct Optional<const N: usize> {
    required: Required,
    values: Option<[Required; N]>,
}
fn main() {
    let mut plain = [Required::One as u8];
    let mut optional = [Required::One as u8];

    let _ = Plain::<0>::access(&plain);
    let _ = Plain::<0>::access_mut(&mut plain);
    let _ = Optional::<0>::access(&optional);
    let _ = Optional::<0>::access_mut(&mut optional).map(|mut root| {
        root.values_mut().set(Some([]))
    });
}
