use zero_schema_macros::zero;

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    First = 1,
    Second = 2,
}

#[zero(crate = zs)]
pub struct Child {
    state: State,
}

#[zero(crate = zs)]
pub struct OptionalRoot {
    maybe: Option<Child>,
}

fn main() {
    let mut storage = zs::schema_buffer!(OptionalRoot);
    let mut root = OptionalRoot::access_mut(storage.as_bytes_mut()).expect("zero sentinel is valid");
    let mut maybe = root.maybe_mut();
    maybe
        .set(Some(Child {
            state: State::First,
        }))
        .expect("initialize optional child");

    let child = maybe.get_mut().expect("present child");
    maybe.set(None).expect("must not overlap the child borrow");
    assert_eq!(child.state(), State::First);
}
