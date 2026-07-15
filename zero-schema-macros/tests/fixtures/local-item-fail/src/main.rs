use zero_schema_macros::zero;

fn main() {
    #[zero(crate = zs)]
    struct Local {
        value: u8,
    }

    let _ = Local { value: 1 };
}
