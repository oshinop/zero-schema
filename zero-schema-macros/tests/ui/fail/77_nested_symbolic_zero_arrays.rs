use zero_schema_macros::zero;

type U16CStr = zs::__private::U16CStr;

// A symbolic length is checked at every nesting level, not only the outermost.
// `tag` keeps each root nonzero-sized so only the array assertion can fire.
#[zero(crate = zs)]
pub struct InnerSymbolic<const N: usize> {
    tag: u8,
    grid: [[u8; N]; 2],
}

#[zero(crate = zs)]
pub struct BothSymbolic<const R: usize, const C: usize> {
    tag: u8,
    grid: [[u8; C]; R],
}

#[zero(crate = zs)]
pub struct StringsSymbolic<'a, const N: usize> {
    tag: u8,
    #[zero(capacity = 2)]
    names: [[&'a U16CStr; N]; 2],
}

fn main() {
    let inner = [0_u8; 1];
    let _ = InnerSymbolic::<0>::access(&inner);
    let both = [0_u8; 3];
    let _ = BothSymbolic::<2, 0>::access(&both);
    let strings = [0_u8; 1];
    let _ = StringsSymbolic::<0>::access(&strings);
}
