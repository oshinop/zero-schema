use zero_schema_macros::zero;

#[zero]
struct Ambiguous<'a, 'b> {
    #[zero(capacity = 4)]
    left: &'a str,
    #[zero(capacity = 4)]
    right: &'b str,
}

fn main() {}
