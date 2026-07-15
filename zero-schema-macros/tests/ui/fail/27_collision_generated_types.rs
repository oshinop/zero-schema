use zero_schema_macros::zero;

struct TypeCollisionRef;
struct TypeCollisionMut;
struct TypeCollisionPatch;
struct TypeCollisionAccessError;
struct TypeCollisionMutationError;

#[zero(crate = zs)]
struct TypeCollision {
    value: u8,
}

fn main() {}
