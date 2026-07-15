#![deny(warnings)]

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
    #[zero(align = 8)]
    maybe: Option<Child>,
}

fn main() {
    let mut storage = zs::schema_buffer!(OptionalRoot);
    let _: Result<OptionalRootRef<'_>, OptionalRootAccessError> =
        OptionalRoot::access(storage.as_bytes());
    let _: (usize, usize, usize) = (
        OptionalRoot::SCHEMA_SIZE,
        OptionalRoot::SCHEMA_ALIGN,
        OptionalRoot::SCHEMA_STRIDE,
    );
    let _ = OptionalRoot::LAYOUT;

    {
        let mut root: OptionalRootMut<'_> = OptionalRoot::access_mut(storage.as_bytes_mut())
            .expect("zero sentinel is a valid absent value");
        let _: OptionalRoot = root.copy_into();

        let mut maybe = root.maybe_mut();
        let _: Option<ChildRef<'_>> = maybe.get();
        assert!(maybe.get().is_none());
        maybe
            .set(Some(Child {
                state: State::First,
            }))
            .expect("public OptionMut initializes a present value");
        assert_eq!(
            maybe
                .get()
                .expect("present optional field exposes a standard Option getter")
                .state(),
            State::First
        );
        {
            let mut child: ChildMut<'_> = maybe
                .get_mut()
                .expect("present optional field exposes a standard Option mut getter");
            child
                .state_mut()
                .set(State::Second)
                .expect("child mutation remains callable through OptionMut");
        }
        assert_eq!(
            maybe
                .get()
                .expect("short mutable borrow released before re-observation")
                .state(),
            State::Second
        );
        maybe
            .set(None)
            .expect("public OptionMut clears the complete optional storage");
        drop(maybe);

        let patch: OptionalRootPatch = Default::default();
        let _: Result<(), OptionalRootMutationError> = root.copy_from(&patch);
    }

    let root: OptionalRootRef<'_> = OptionalRoot::access(storage.as_bytes())
        .expect("cleared optional field remains valid");
    let _: Option<ChildRef<'_>> = root.maybe();
    assert!(root.maybe().is_none());
    let _: OptionalRoot = root.copy_into();
}
