use zero_schema_macros::zero;

#[zero]
struct MissingSibling {
    #[zero(tag_field = missing)]
    payload: Payload,
}

struct Payload;

fn main() {}
