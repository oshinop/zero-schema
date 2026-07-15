//! External-tagged payload selection for generated union capabilities.

use core::{mem, ops::Range};

use zerocopy::{FromBytes, Immutable, KnownLayout};

use crate::{
    __private::{
        ExclusiveInput, OwnerAdapter, RootInputAccess, SchemaSupport, SharedInput,
        TaggedPayloadPatch, TaggedPayloadSupport,
    },
    error::LayoutError,
    mutation::checked_range,
};

/// A proved logical external tag paired with the selected payload location.
///
/// This is a generated-code building block, not a user capability. It retains
/// no tag storage reference and offers no tag mutation: the tag is an already
/// decoded logical value and the payload is the sole selected location. It
/// never forms a union member reference.
#[doc(hidden)]
pub struct TaggedRefSelection<'wire, Payload>
where
    Payload: TaggedPayloadSupport,
{
    tag: Payload::Tag,
    payload: SharedInput<'wire, Payload::Wire>,
}

impl<'wire, Payload> Copy for TaggedRefSelection<'wire, Payload> where Payload: TaggedPayloadSupport {}

impl<'wire, Payload> Clone for TaggedRefSelection<'wire, Payload>
where
    Payload: TaggedPayloadSupport,
{
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'wire, Payload> TaggedRefSelection<'wire, Payload>
where
    Payload: TaggedPayloadSupport,
{
    /// Selects the payload at a checked generated offset and proves exactly the
    /// variant named by the already validated logical sibling tag.
    #[doc(hidden)]
    #[inline]
    pub fn prove_at<Root>(
        root: SharedInput<'wire, Root::Wire>,
        tag: Payload::Tag,
        payload_offset: usize,
        _: Root::Token,
    ) -> Result<Self, <Payload::Owner as OwnerAdapter>::AccessError>
    where
        Root: SchemaSupport,
    {
        let payload = root
            .subrange::<Payload::Wire>(payload_offset)
            .map_err(<Payload::Owner as OwnerAdapter>::access_layout)?;
        Payload::validate_selected(tag, payload)?;
        Ok(Self { tag, payload })
    }

    /// Completes a selected-payload proof only inside the payload's generated
    /// support module. This is used to rematerialize a retained capability.
    #[doc(hidden)]
    #[inline]
    pub fn prove_selected(
        tag: Payload::Tag,
        payload: SharedInput<'wire, Payload::Wire>,
        _: Payload::Token,
    ) -> Result<Self, <Payload::Owner as OwnerAdapter>::AccessError> {
        Payload::validate_selected(tag, payload)?;
        Ok(Self { tag, payload })
    }

    /// Returns the proved logical tag. This value has no independent storage
    /// capability and cannot mutate the containing record's external tag.
    #[doc(hidden)]
    #[inline]
    pub const fn tag(&self) -> Payload::Tag {
        self.tag
    }

    pub fn into_parts(
        self,
        _: Payload::Token,
    ) -> (Payload::Tag, SharedInput<'wire, Payload::Wire>) {
        (self.tag, self.payload)
    }

    /// Consumes a copy of this exact selection to create its generated read
    /// capability. The trait receives no free tag or payload input.
    #[doc(hidden)]
    #[inline]
    pub fn make_ref(&self) -> Payload::Ref<'wire> {
        Payload::make_ref(*self)
    }

    /// Materializes only the selected logical payload variant.
    #[doc(hidden)]
    #[inline]
    pub fn materialize(&self) -> Payload::Logical<'wire> {
        Payload::materialize_selected(*self)
    }
}

/// An exclusive selection of one already proved external-tagged payload.
///
/// It retains only the decoded tag and a short payload reborrow. It has no tag
/// storage capability, so same-variant field chains can mutate payload fields
/// but cannot independently mutate the containing record's sibling tag.
#[doc(hidden)]
pub struct TaggedMutSelection<'view, Payload>
where
    Payload: TaggedPayloadSupport,
{
    tag: Payload::Tag,
    payload: ExclusiveInput<'view, Payload::Wire>,
}

impl<'view, Payload> TaggedMutSelection<'view, Payload>
where
    Payload: TaggedPayloadSupport,
{
    pub fn prove_at<'parent, Root>(
        root: &'view mut ExclusiveInput<'parent, Root::Wire>,
        tag: Payload::Tag,
        payload_offset: usize,
        _: Root::Token,
    ) -> Result<Self, <Payload::Owner as OwnerAdapter>::AccessError>
    where
        Root: SchemaSupport,
    {
        let payload = root
            .subrange_mut::<Payload::Wire>(payload_offset)
            .map_err(<Payload::Owner as OwnerAdapter>::access_layout)?;
        Payload::validate_selected(tag, payload.shared())?;
        Ok(Self { tag, payload })
    }

    /// Returns the selected logical tag without exposing its sibling storage.
    #[doc(hidden)]
    #[inline]
    pub const fn tag(&self) -> Payload::Tag {
        self.tag
    }

    pub fn into_parts(
        self,
        _: Payload::Token,
    ) -> (Payload::Tag, ExclusiveInput<'view, Payload::Wire>) {
        (self.tag, self.payload)
    }

    /// Consumes this short reborrow to create the generated payload mutation
    /// capability for the selected variant only.
    #[doc(hidden)]
    #[inline]
    pub fn make_mut(self) -> Payload::Mut<'view> {
        Payload::make_mut(self)
    }
}

/// Commits a preflighted tagged-payload patch before a generated final tag store.
///
/// Generated containing-record patch code must run all logical preflight first.
/// This helper then validates both static ranges before the payload commit, runs
/// only the payload commit, and invokes `store_tag` last with a separate exact
/// tag-field input. No payload API can see tag storage.
#[doc(hidden)]
#[inline]
pub fn commit_payload_before_tag<Root, Payload, TagWire, P, StoreTag>(
    root: &mut ExclusiveInput<'_, Root>,
    tag_offset: usize,
    payload_offset: usize,
    patch: &P,
    _: Root::Token,
    store_tag: StoreTag,
) -> Result<(), <Payload::Owner as OwnerAdapter>::MutationError>
where
    Root: RootInputAccess + FromBytes + KnownLayout + Immutable,
    Payload: TaggedPayloadSupport,
    TagWire: FromBytes + KnownLayout + Immutable,
    P: TaggedPayloadPatch<Payload>,
    StoreTag: FnOnce(ExclusiveInput<'_, TagWire>),
{
    checked_tagged_ranges(
        mem::size_of::<Root>(),
        tag_offset,
        mem::size_of::<TagWire>(),
        payload_offset,
        mem::size_of::<Payload::Wire>(),
    )
    .map_err(<Payload::Owner as OwnerAdapter>::mutation_layout)?;

    let payload = match root.subrange_mut::<Payload::Wire>(payload_offset) {
        Ok(payload) => payload,
        Err(_) => unreachable!("preflighted generated payload range remains selectable"),
    };
    let token = Payload::input_token(&payload);
    Payload::commit_patch(payload, patch, token);

    let tag = match root.subrange_mut::<TagWire>(tag_offset) {
        Ok(tag) => tag,
        Err(_) => unreachable!("preflighted generated tag range remains selectable"),
    };
    store_tag(tag);
    Ok(())
}

/// Commits a preflighted borrowed logical payload before storing the external
/// tag. This is the direct-logical counterpart to
/// [`commit_payload_before_tag`]; it deliberately accepts only a payload
/// callback, never tag storage.
#[doc(hidden)]
#[inline]
pub fn commit_payload_before_tag_with<Root, Payload, TagWire, CommitPayload, StoreTag>(
    root: &mut ExclusiveInput<'_, Root>,
    tag_offset: usize,
    payload_offset: usize,
    commit_payload: CommitPayload,
    _: Root::Token,
    store_tag: StoreTag,
) -> Result<(), <Payload::Owner as OwnerAdapter>::MutationError>
where
    Root: RootInputAccess + FromBytes + KnownLayout + Immutable,
    Payload: TaggedPayloadSupport,
    TagWire: FromBytes + KnownLayout + Immutable,
    CommitPayload: FnOnce(ExclusiveInput<'_, Payload::Wire>),
    StoreTag: FnOnce(ExclusiveInput<'_, TagWire>),
{
    checked_tagged_ranges(
        mem::size_of::<Root>(),
        tag_offset,
        mem::size_of::<TagWire>(),
        payload_offset,
        mem::size_of::<Payload::Wire>(),
    )
    .map_err(<Payload::Owner as OwnerAdapter>::mutation_layout)?;

    let payload = match root.subrange_mut::<Payload::Wire>(payload_offset) {
        Ok(payload) => payload,
        Err(_) => unreachable!("preflighted generated payload range remains selectable"),
    };
    commit_payload(payload);

    let tag = match root.subrange_mut::<TagWire>(tag_offset) {
        Ok(tag) => tag,
        Err(_) => unreachable!("preflighted generated tag range remains selectable"),
    };
    store_tag(tag);
    Ok(())
}

/// Selects the exact storage range for a payload associated with a proved tag.
///
/// Tag decoding and variant selection remain generated, schema-specific work;
/// this helper only preserves checked offset arithmetic for the selected payload.
#[doc(hidden)]
#[inline]
pub fn checked_payload_range(
    available: usize,
    payload_offset: usize,
    payload_size: usize,
) -> Result<Range<usize>, LayoutError> {
    checked_range(available, payload_offset, payload_size)
}

/// Checks both a tag field and payload field range before either is accessed.
#[doc(hidden)]
#[inline]
pub fn checked_tagged_ranges(
    available: usize,
    tag_offset: usize,
    tag_size: usize,
    payload_offset: usize,
    payload_size: usize,
) -> Result<(Range<usize>, Range<usize>), LayoutError> {
    let tag = checked_range(available, tag_offset, tag_size)?;
    let payload = checked_payload_range(available, payload_offset, payload_size)?;
    Ok((tag, payload))
}

#[cfg(test)]
mod tests {
    use core::{
        fmt,
        sync::atomic::{AtomicU8, Ordering},
    };

    use super::*;
    use crate::{
        __private::{
            ExclusiveInput, InputAccess, OwnerAdapter, RootInputAccess, ScalarEnumSupport,
            SchemaSupport, TaggedPayloadPatch, TaggedPayloadSupport,
        },
        error::{ErrorKind, ErrorPathSegment, SchemaError},
    };

    fn release<T>(value: T) {
        drop(value);
    }

    #[derive(Debug)]
    struct TestError;

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test error")
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::UnknownUnionTag
        }

        fn schema(&self) -> &'static str {
            "TaggedTest"
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            None
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }

        fn __fmt_leaf(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test error")
        }
    }

    struct TestOwner;

    impl OwnerAdapter for TestOwner {
        type AccessError = TestError;
        type MutationError = TestError;

        fn access_layout(_: LayoutError) -> Self::AccessError {
            TestError
        }

        fn mutation_layout(_: LayoutError) -> Self::MutationError {
            TestError
        }
    }

    struct RootSupport;
    #[derive(Clone, Copy)]
    pub struct RootToken;
    impl InputAccess for RootSupport {
        type Token = RootToken;
    }
    impl RootInputAccess for [u8; 4] {
        type Token = RootToken;
    }
    impl SchemaSupport for RootSupport {
        type Wire = [u8; 4];
        type Owner = TestOwner;
        type Ref<'wire> = ();
        type Mut<'wire> = ();
        fn validate<'wire>(_: SharedInput<'wire, Self::Wire>) -> Result<(), TestError> {
            Ok(())
        }
        fn make_ref<'wire>(
            _: crate::__private::ProvedShared<'wire, Self, Self::Wire>,
        ) -> Self::Ref<'wire> {
        }
        fn make_mut<'wire>(
            _: crate::__private::ProvedExclusive<'wire, Self, Self::Wire>,
        ) -> Self::Mut<'wire> {
        }
        fn input_token(_: &ExclusiveInput<'_, Self::Wire>) -> Self::Token {
            RootToken
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Tag {
        File,
        Memory,
    }

    struct TagSupport;

    #[derive(Clone, Copy)]
    struct TagToken;
    impl InputAccess for TagSupport {
        type Token = TagToken;
    }

    impl SchemaSupport for TagSupport {
        type Wire = u8;
        type Owner = TestOwner;
        type Ref<'wire> = Tag;
        type Mut<'wire> = ();

        fn validate<'wire>(input: SharedInput<'wire, Self::Wire>) -> Result<(), TestError> {
            Self::from_raw(Self::raw(input))
                .map(|_| ())
                .ok_or(TestError)
        }

        fn make_ref<'wire>(
            proof: crate::__private::ProvedShared<'wire, Self, Self::Wire>,
        ) -> Self::Ref<'wire> {
            Self::from_raw(Self::raw(proof.into_input(TagToken)))
                .expect("validated scalar test token")
        }

        fn make_mut<'wire>(
            _: crate::__private::ProvedExclusive<'wire, Self, Self::Wire>,
        ) -> Self::Mut<'wire> {
        }
        fn input_token(_: &ExclusiveInput<'_, Self::Wire>) -> Self::Token {
            TagToken
        }
    }

    impl ScalarEnumSupport for TagSupport {
        type Raw = u8;
        type Value = Tag;

        fn raw(input: SharedInput<'_, Self::Wire>) -> Self::Raw {
            input.read_copy::<u8>(0).expect("exact tag storage")
        }

        fn from_raw(raw: Self::Raw) -> Option<Self::Value> {
            match raw {
                1 => Some(Tag::File),
                2 => Some(Tag::Memory),
                _ => None,
            }
        }

        fn to_raw(value: Self::Value) -> Self::Raw {
            match value {
                Tag::File => 1,
                Tag::Memory => 2,
            }
        }

        fn commit(
            mut input: ExclusiveInput<'_, Self::Wire>,
            value: Self::Value,
            token: Self::Token,
        ) {
            input.subrange_bytes_mut::<Self>(0, 1, token).unwrap()[0] = Self::to_raw(value);
        }
    }

    struct PayloadSupport;

    #[derive(Clone, Copy)]
    struct PayloadToken;
    impl InputAccess for PayloadSupport {
        type Token = PayloadToken;
    }

    struct PayloadMut<'wire>(ExclusiveInput<'wire, [u8; 2]>);

    impl PayloadMut<'_> {
        fn set_first(&mut self, value: u8) {
            self.0
                .subrange_bytes_mut::<PayloadSupport>(0, 1, PayloadToken)
                .unwrap()[0] = value;
        }
    }

    static COMMIT_STEP: AtomicU8 = AtomicU8::new(0);

    struct PayloadPatch {
        tag: Tag,
        first: u8,
    }

    impl TaggedPayloadPatch<PayloadSupport> for PayloadPatch {
        fn tag(&self) -> Tag {
            self.tag
        }

        fn is_complete(&self) -> bool {
            true
        }

        fn preflight<'wire>(
            &self,
            _: Tag,
            _: SharedInput<'wire, [u8; 2]>,
        ) -> Result<(), TestError> {
            Ok(())
        }

        fn commit<'wire>(&self, mut payload: ExclusiveInput<'wire, [u8; 2]>, token: PayloadToken) {
            assert_eq!(COMMIT_STEP.swap(1, Ordering::SeqCst), 0);
            payload
                .subrange_bytes_mut::<PayloadSupport>(0, 1, token)
                .unwrap()[0] = self.first;
        }
    }

    impl TaggedPayloadSupport for PayloadSupport {
        type Tag = Tag;
        type Wire = [u8; 2];
        type Owner = TestOwner;
        type Logical<'wire> = (&'wire u8, &'wire u8);
        type Ref<'wire> = (&'wire u8, &'wire u8);
        type Mut<'wire> = PayloadMut<'wire>;

        fn validate_selected<'wire>(
            tag: Self::Tag,
            payload: SharedInput<'wire, Self::Wire>,
        ) -> Result<(), TestError> {
            let first = payload.read_copy::<u8>(0).expect("exact payload byte");
            let second = payload.read_copy::<u8>(1).expect("exact payload byte");
            match tag {
                Tag::File if first == 7 => Ok(()),
                Tag::Memory if second == 8 => Ok(()),
                _ => Err(TestError),
            }
        }
        fn input_token(_: &ExclusiveInput<'_, Self::Wire>) -> Self::Token {
            PayloadToken
        }

        fn make_ref<'wire>(selection: TaggedRefSelection<'wire, Self>) -> Self::Ref<'wire> {
            let (_, payload) = selection.into_parts(PayloadToken);
            let bytes = payload
                .subrange_bytes::<PayloadSupport>(0, 2, PayloadToken)
                .expect("exact payload bytes");
            (&bytes[0], &bytes[1])
        }

        fn make_mut<'wire>(selection: TaggedMutSelection<'wire, Self>) -> Self::Mut<'wire> {
            let (_, payload) = selection.into_parts(PayloadToken);
            PayloadMut(payload)
        }

        fn materialize_selected<'wire>(
            selection: TaggedRefSelection<'wire, Self>,
        ) -> Self::Logical<'wire> {
            Self::make_ref(selection)
        }

        fn patch_tag<P>(patch: &P) -> Self::Tag
        where
            P: TaggedPayloadPatch<Self>,
        {
            patch.tag()
        }

        fn patch_is_complete<P>(patch: &P) -> bool
        where
            P: TaggedPayloadPatch<Self>,
        {
            patch.is_complete()
        }

        fn preflight_patch<'wire, P>(
            current_tag: Self::Tag,
            payload: SharedInput<'wire, Self::Wire>,
            patch: &P,
        ) -> Result<(), TestError>
        where
            P: TaggedPayloadPatch<Self>,
        {
            patch.preflight(current_tag, payload)
        }

        fn preflight_patch_init<'wire, P>(
            payload: SharedInput<'wire, Self::Wire>,
            patch: &P,
        ) -> Result<(), TestError>
        where
            P: TaggedPayloadPatch<Self>,
        {
            patch.preflight_init(payload)
        }

        fn commit_patch<'wire, P>(
            payload: ExclusiveInput<'wire, Self::Wire>,
            patch: &P,
            token: Self::Token,
        ) where
            P: TaggedPayloadPatch<Self>,
        {
            patch.commit(payload, token)
        }
    }

    #[test]
    fn scalar_and_external_tag_select_only_the_proved_payload() {
        let bytes = [2_u8, 0xa5, 7, 8];
        let root = SharedInput::<[u8; 4]>::from_checked(&bytes).unwrap();
        let tag = TagSupport::from_raw(TagSupport::raw(root.subrange::<u8>(0).unwrap())).unwrap();
        assert_eq!(tag, Tag::Memory);
        assert_eq!(TagSupport::to_raw(tag), 2);
        assert_eq!(TagSupport::from_raw(3), None);

        let selection =
            TaggedRefSelection::<PayloadSupport>::prove_at::<RootSupport>(root, tag, 2, RootToken)
                .unwrap();
        assert_eq!(selection.tag(), Tag::Memory);
        assert_eq!(*selection.make_ref().0, 7);
        assert_eq!(*selection.materialize().1, 8);

        assert!(
            TaggedRefSelection::<PayloadSupport>::prove_at::<RootSupport>(
                root,
                Tag::File,
                2,
                RootToken
            )
            .is_ok()
        );
        assert!(
            TaggedRefSelection::<PayloadSupport>::prove_at::<RootSupport>(
                root,
                Tag::File,
                1,
                RootToken
            )
            .is_err()
        );
    }

    #[test]
    fn tag_range_overflow_precedes_bounds() {
        assert_eq!(
            checked_tagged_ranges(1, usize::MAX, 2, 0, 0),
            Err(LayoutError::OffsetOverflow)
        );
    }

    #[test]
    fn selected_ranges_are_exact() {
        assert_eq!(
            checked_tagged_ranges(12, 0, 1, 4, 8).unwrap(),
            (0..1, 4..12)
        );
    }

    #[test]
    fn selected_mutation_cannot_access_tag_and_patch_writes_payload_first() {
        let mut selected_bytes = [2_u8, 0xa5, 7, 8];
        let mut root = ExclusiveInput::<[u8; 4]>::from_checked(&mut selected_bytes).unwrap();
        {
            let mut selected = TaggedMutSelection::<PayloadSupport>::prove_at::<RootSupport>(
                &mut root,
                Tag::Memory,
                2,
                RootToken,
            )
            .unwrap()
            .make_mut();
            selected.set_first(9);
        }
        assert_eq!(
            root.subrange_bytes::<RootSupport>(0, 1, RootToken).unwrap(),
            &[2]
        );
        release(root);
        assert_eq!(selected_bytes, [2, 0xa5, 9, 8]);

        let mut patched_bytes = [1_u8, 0xa5, 7, 8];
        let mut root = ExclusiveInput::<[u8; 4]>::from_checked(&mut patched_bytes).unwrap();
        let patch = PayloadPatch {
            tag: Tag::Memory,
            first: 4,
        };
        COMMIT_STEP.store(0, Ordering::SeqCst);
        commit_payload_before_tag::<[u8; 4], PayloadSupport, u8, _, _>(
            &mut root,
            0,
            2,
            &patch,
            RootToken,
            |mut tag| {
                assert_eq!(COMMIT_STEP.swap(2, Ordering::SeqCst), 1);
                tag.subrange_bytes_mut::<TagSupport>(0, 1, TagToken)
                    .unwrap()[0] = 2;
            },
        )
        .unwrap();
        release(root);
        assert_eq!(COMMIT_STEP.load(Ordering::SeqCst), 2);
        assert_eq!(patched_bytes, [2, 0xa5, 4, 8]);
    }
}
