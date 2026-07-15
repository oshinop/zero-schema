use zero_schema_macros::zero;

#[zero(crate = zs)]
struct MissingTagFieldValue {
    #[zero(tag_field =)]
    payload: Payload,
}

struct Payload;

fn main() {}
