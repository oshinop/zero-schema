//! Zero-copy fixed-array read capabilities and checked element selection.

use core::{iter::FusedIterator, marker::PhantomData, ops::Range};

use crate::{
    __private::{ArrayElementAdapter, ExclusiveInput, InputAccess, OwnerAdapter, SharedInput},
    error::{ErrorKind, LayoutError, SchemaError},
    mutation::checked_range,
};

/// Marker used only as the default hidden adapter parameter for `ArrayRef`.
///
/// Generated accessors always supply their concrete generated adapter. The
/// default preserves the conventional three-parameter spelling in diagnostics
/// without inventing a universal wire interpretation for arbitrary `LogicalT`.
#[doc(hidden)]
pub enum UnspecifiedArrayAdapter {}

#[doc(hidden)]
#[derive(Debug)]
pub struct UnspecifiedArrayError;

impl core::fmt::Display for UnspecifiedArrayError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("unspecified array adapter")
    }
}

impl core::error::Error for UnspecifiedArrayError {}

impl SchemaError for UnspecifiedArrayError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Layout
    }
    fn schema(&self) -> &'static str {
        "unspecified array adapter"
    }
    fn segment(&self) -> Option<crate::ErrorPathSegment> {
        None
    }
    fn child(&self) -> Option<&dyn SchemaError> {
        None
    }
    fn __fmt_leaf(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, formatter)
    }
}

#[doc(hidden)]
pub struct UnspecifiedArrayOwner;

impl OwnerAdapter for UnspecifiedArrayOwner {
    type AccessError = UnspecifiedArrayError;
    type MutationError = UnspecifiedArrayError;

    fn access_layout(_: LayoutError) -> Self::AccessError {
        UnspecifiedArrayError
    }
    fn mutation_layout(_: LayoutError) -> Self::MutationError {
        UnspecifiedArrayError
    }
}

impl InputAccess for UnspecifiedArrayAdapter {
    type Token = ();
}

impl ArrayElementAdapter for UnspecifiedArrayAdapter {
    type Wire = u8;
    type ArrayWire<const N: usize> = [u8; N];
    type Owner = UnspecifiedArrayOwner;
    type Read<'wire> = ();
    type Value<'source> = ();
    type Mut<'wire> = ();
    const STRIDE: usize = 1;

    fn prove<'wire>(
        _: usize,
        _: SharedInput<'wire, Self::Wire>,
    ) -> Result<(), UnspecifiedArrayError> {
        Err(UnspecifiedArrayError)
    }
    fn read<'wire>(
        _: usize,
        _: SharedInput<'wire, Self::Wire>,
    ) -> Result<Self::Read<'wire>, UnspecifiedArrayError> {
        Err(UnspecifiedArrayError)
    }
    fn make_mut<'wire>(
        _: usize,
        _: ExclusiveInput<'wire, Self::Wire>,
        _: Self::Token,
    ) -> Result<Self::Mut<'wire>, UnspecifiedArrayError> {
        Err(UnspecifiedArrayError)
    }
    fn preflight<'wire, 'value>(
        _: usize,
        _: SharedInput<'wire, Self::Wire>,
        _: &Self::Value<'value>,
    ) -> Result<(), UnspecifiedArrayError> {
        Err(UnspecifiedArrayError)
    }
    fn commit<'wire, 'value>(
        _: usize,
        _: ExclusiveInput<'wire, Self::Wire>,
        _: &Self::Value<'value>,
        _: Self::Token,
    ) {
    }
    fn index_error(_: usize, _: usize) -> UnspecifiedArrayError {
        UnspecifiedArrayError
    }
    fn length_error(_: usize, _: usize) -> UnspecifiedArrayError {
        UnspecifiedArrayError
    }
}

/// An O(1)-state, zero-copy view of a fully proved fixed wire array.
///
/// The generated adapter is intentionally a doc-hidden implementation detail.
/// This public capability exposes only logical elements: it never returns wire
/// values, byte slices, pointers, or a mutable element location.
pub struct ArrayRef<'wire, LogicalT, const N: usize, Adapter = UnspecifiedArrayAdapter>
where
    Adapter: ArrayElementAdapter,
{
    input: SharedInput<'wire, Adapter::ArrayWire<N>>,
    _adapter: PhantomData<fn() -> (LogicalT, Adapter)>,
}

impl<'wire, LogicalT, const N: usize, Adapter> Copy for ArrayRef<'wire, LogicalT, N, Adapter> where
    Adapter: ArrayElementAdapter
{
}

impl<'wire, LogicalT, const N: usize, Adapter> Clone for ArrayRef<'wire, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter,
{
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'wire, LogicalT, const N: usize, Adapter> ArrayRef<'wire, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter<Read<'wire> = LogicalT>,
{
    /// Generated root proof walkers call this at access time. Every element is
    /// validated in increasing index order before the view is constructed.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: SharedInput<'wire, Adapter::ArrayWire<N>>,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        let view = Self {
            input,
            _adapter: PhantomData,
        };
        for index in 0..N {
            let element = view
                .element_input_result(index)
                .map_err(<Adapter::Owner as OwnerAdapter>::access_layout)?;
            Adapter::prove(index, element)?;
        }
        Ok(view)
    }

    /// Returns one logical element or `None` when `index` is outside `N`.
    #[inline]
    pub fn get(&self, index: usize) -> Option<LogicalT> {
        if index >= N {
            return None;
        }
        let input = match self.element_input_result(index) {
            Ok(input) => input,
            Err(_) => unreachable!("a proved array retains every element range"),
        };
        match Adapter::read(index, input) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved array retains every valid element"),
        }
    }

    /// Iterates logical elements in increasing index order.
    #[inline]
    pub fn iter(&self) -> ArrayRefIter<'wire, LogicalT, N, Adapter> {
        ArrayRefIter {
            view: *self,
            next: 0,
        }
    }

    /// Materializes all logical elements without copying padding between wires.
    #[inline]
    pub fn copy_into(&self) -> [LogicalT; N] {
        core::array::from_fn(|index| match self.get(index) {
            Some(element) => element,
            None => unreachable!("a proved array has every declared element"),
        })
    }

    #[inline]
    fn element_input_result(
        &self,
        index: usize,
    ) -> Result<SharedInput<'wire, Adapter::Wire>, LayoutError> {
        let offset = checked_element_offset(index, Adapter::STRIDE)?;
        self.input.subrange(offset)
    }
}

/// An O(1)-state exclusive capability for one fully proved fixed wire array.
///
/// It never yields mutable storage. `get_mut` yields the generated logical
/// element mutation capability for one short selected reborrow; full-array
/// transfer preflights every source element before its first write.
///
/// A row of a nested array additionally remembers the flat position of its
/// first element among all same-depth elements of the enclosing arrays. Failing
/// operations pass that flat position to the generated element adapter so an
/// error path names every enclosing index; a field-level array has base zero.
pub struct ArrayMut<'view, LogicalT, const N: usize, Adapter>
where
    Adapter: ArrayElementAdapter,
{
    input: ExclusiveInput<'view, Adapter::ArrayWire<N>>,
    token: Adapter::Token,
    base: usize,
    _adapter: PhantomData<fn() -> (LogicalT, Adapter)>,
}

impl<'view, LogicalT, const N: usize, Adapter> ArrayMut<'view, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter,
{
    /// Validates every exact element in increasing order before constructing an
    /// exclusive array capability.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: ExclusiveInput<'view, Adapter::ArrayWire<N>>,
        token: Adapter::Token,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        Self::prove_at(input, token, 0)
    }

    /// Validates a nested row whose first element has flat position `base`.
    #[doc(hidden)]
    #[inline]
    pub fn prove_at(
        input: ExclusiveInput<'view, Adapter::ArrayWire<N>>,
        token: Adapter::Token,
        base: usize,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        for index in 0..N {
            let offset = checked_element_offset(index, Adapter::STRIDE)
                .map_err(<Adapter::Owner as OwnerAdapter>::access_layout)?;
            let element = input
                .shared()
                .subrange::<Adapter::Wire>(offset)
                .map_err(<Adapter::Owner as OwnerAdapter>::access_layout)?;
            Adapter::prove(base.saturating_add(index), element)?;
        }
        Ok(Self {
            input,
            token,
            base,
            _adapter: PhantomData,
        })
    }

    /// Returns one logical read observation or `None` when `index` is outside `N`.
    #[inline]
    pub fn get(&self, index: usize) -> Option<Adapter::Read<'_>> {
        if index >= N {
            return None;
        }
        let input = match self.element_input_result(index) {
            Ok(input) => input,
            Err(_) => unreachable!("a proved array retains every element range"),
        };
        match Adapter::read(index, input) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved array retains every valid element"),
        }
    }

    /// Returns a field-local mutable element capability or `None` out of bounds.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<Adapter::Mut<'_>> {
        if index >= N {
            return None;
        }
        let token = self.token;
        let position = self.flat(index);
        let input = match self.element_input_mut_result(index) {
            Ok(input) => input,
            Err(_) => unreachable!("a proved array retains every element range"),
        };
        match Adapter::make_mut(position, input, token) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved array retains every valid element"),
        }
    }

    /// Iterates logical elements in increasing index order through shared short
    /// reborrows of this exclusive array capability.
    #[inline]
    pub fn iter(&self) -> ArrayMutIter<'_, LogicalT, N, Adapter> {
        ArrayMutIter {
            input: self.input.shared(),
            next: 0,
            _adapter: PhantomData,
        }
    }

    /// Materializes all logical read observations without copying padding between wires.
    #[inline]
    pub fn copy_into(&self) -> [Adapter::Read<'_>; N] {
        core::array::from_fn(|index| match self.get(index) {
            Some(element) => element,
            None => unreachable!("a proved array has every declared element"),
        })
    }

    /// Replaces one element after bounds and source preflight.
    #[inline]
    pub fn set<'value>(
        &mut self,
        index: usize,
        value: Adapter::Value<'value>,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        if index >= N {
            return Err(Adapter::index_error(index, N));
        }
        let proved = self
            .element_input_result(index)
            .map_err(<Adapter::Owner as OwnerAdapter>::mutation_layout)?;
        Adapter::preflight(self.flat(index), proved, &value)?;
        let token = self.token;
        let position = self.flat(index);
        let input = self
            .element_input_mut_result(index)
            .map_err(<Adapter::Owner as OwnerAdapter>::mutation_layout)?;
        Adapter::commit(position, input, &value, token);
        Ok(())
    }

    /// Replaces exactly `N` elements with two-pass atomicity.
    ///
    /// The source length and every source value are validated in increasing
    /// index order before any selected destination range is written. Commit is
    /// then increasing-index and infallible by the adapter contract.
    #[inline]
    pub fn copy_from<'value>(
        &mut self,
        values: &[Adapter::Value<'value>],
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        if values.len() != N {
            return Err(Adapter::length_error(values.len(), N));
        }

        self.preflight_element_locations()?;
        for (index, value) in values.iter().enumerate() {
            let input = match self.element_input_result(index) {
                Ok(input) => input,
                Err(_) => unreachable!("preflighted exact array ranges remain selectable"),
            };
            Adapter::preflight(self.flat(index), input, value)?;
        }

        for (index, value) in values.iter().enumerate() {
            let token = self.token;
            let position = self.flat(index);
            let input = match self.element_input_mut_result(index) {
                Ok(input) => input,
                Err(_) => unreachable!("preflighted exact array ranges remain selectable"),
            };
            Adapter::commit(position, input, value, token);
        }
        Ok(())
    }

    /// Flat position of local `index` among same-depth elements of every
    /// enclosing array. Only fallible operations use it, so error paths can
    /// name all enclosing indices; reads receive the local position.
    #[inline]
    fn flat(&self, index: usize) -> usize {
        self.base.saturating_add(index)
    }

    #[inline]
    fn preflight_element_locations(
        &self,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        for index in 0..N {
            self.element_input_result(index)
                .map_err(<Adapter::Owner as OwnerAdapter>::mutation_layout)?;
        }
        Ok(())
    }

    #[inline]
    fn element_input_result(
        &self,
        index: usize,
    ) -> Result<SharedInput<'_, Adapter::Wire>, LayoutError> {
        let offset = checked_element_offset(index, Adapter::STRIDE)?;
        self.input.subrange(offset)
    }

    #[inline]
    fn element_input_mut_result(
        &mut self,
        index: usize,
    ) -> Result<ExclusiveInput<'_, Adapter::Wire>, LayoutError> {
        let offset = checked_element_offset(index, Adapter::STRIDE)?;
        self.input.subrange_mut(offset)
    }
}

/// Exact-size increasing iterator returned by [`ArrayMut::iter`].
pub struct ArrayMutIter<'view, LogicalT, const N: usize, Adapter>
where
    Adapter: ArrayElementAdapter,
{
    input: SharedInput<'view, Adapter::ArrayWire<N>>,
    next: usize,
    _adapter: PhantomData<fn() -> (LogicalT, Adapter)>,
}

impl<'view, LogicalT, const N: usize, Adapter> Iterator
    for ArrayMutIter<'view, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter,
{
    type Item = Adapter::Read<'view>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next == N {
            return None;
        }
        let index = self.next;
        self.next += 1;
        let offset = match checked_element_offset(index, Adapter::STRIDE) {
            Ok(offset) => offset,
            Err(_) => unreachable!("a proved array iterator retains every element range"),
        };
        let input = match self.input.subrange(offset) {
            Ok(input) => input,
            Err(_) => unreachable!("a proved array iterator retains aligned elements"),
        };
        match Adapter::read(index, input) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved array iterator retains valid elements"),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = N - self.next;
        (remaining, Some(remaining))
    }
}

impl<'view, LogicalT, const N: usize, Adapter> ExactSizeIterator
    for ArrayMutIter<'view, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter,
{
}

impl<'view, LogicalT, const N: usize, Adapter> FusedIterator
    for ArrayMutIter<'view, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter,
{
}

/// Exact-size increasing iterator returned by [`ArrayRef::iter`].
pub struct ArrayRefIter<'wire, LogicalT, const N: usize, Adapter = UnspecifiedArrayAdapter>
where
    Adapter: ArrayElementAdapter,
{
    view: ArrayRef<'wire, LogicalT, N, Adapter>,
    next: usize,
}

impl<'wire, LogicalT, const N: usize, Adapter> Iterator
    for ArrayRefIter<'wire, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter<Read<'wire> = LogicalT>,
{
    type Item = LogicalT;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.next == N {
            return None;
        }
        let index = self.next;
        self.next += 1;
        self.view.get(index)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = N - self.next;
        (remaining, Some(remaining))
    }
}

impl<'wire, LogicalT, const N: usize, Adapter> ExactSizeIterator
    for ArrayRefIter<'wire, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter<Read<'wire> = LogicalT>,
{
}

impl<'wire, LogicalT, const N: usize, Adapter> FusedIterator
    for ArrayRefIter<'wire, LogicalT, N, Adapter>
where
    Adapter: ArrayElementAdapter<Read<'wire> = LogicalT>,
{
}

/// Element adapter for an array whose every element is an `N`-element array of
/// `Inner`'s element. It implements C/C++ multidimensional-array layout:
/// the outer stride is the inner array's complete wire size.
///
/// The combinator owns no per-schema state. A row read is the inner
/// [`ArrayRef`]; a row mutation is the inner [`ArrayMut`]; a row value is
/// `[Inner::Value; N]`. Failing operations hand `Inner` the flat position
/// `index * N + j`, so the leaf adapter can name every enclosing index in its
/// structured error path. Preflight visits the whole row in increasing order
/// before any element is committed.
#[doc(hidden)]
pub struct NestedArrayAdapter<Inner, const N: usize>(PhantomData<fn() -> Inner>);

impl<Inner, const N: usize> InputAccess for NestedArrayAdapter<Inner, N>
where
    Inner: ArrayElementAdapter,
{
    type Token = Inner::Token;
}

impl<Inner, const N: usize> NestedArrayAdapter<Inner, N>
where
    Inner: ArrayElementAdapter,
{
    /// Flat position of row element `j` for a row at flat position `index`.
    #[inline]
    fn position(index: usize, j: usize) -> usize {
        index.saturating_mul(N).saturating_add(j)
    }
}

impl<Inner, const N: usize> ArrayElementAdapter for NestedArrayAdapter<Inner, N>
where
    Inner: ArrayElementAdapter,
{
    type Wire = Inner::ArrayWire<N>;
    type ArrayWire<const M: usize> = [Inner::ArrayWire<N>; M];
    type Owner = Inner::Owner;
    type Read<'wire> = ArrayRef<'wire, Inner::Read<'wire>, N, Inner>;
    type Value<'value> = [Inner::Value<'value>; N];
    type Mut<'wire> = ArrayMut<'wire, Inner::Read<'wire>, N, Inner>;

    const STRIDE: usize = core::mem::size_of::<Inner::ArrayWire<N>>();

    fn prove<'wire>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::AccessError> {
        for j in 0..N {
            let offset = checked_element_offset(j, Inner::STRIDE)
                .map_err(<Self::Owner as OwnerAdapter>::access_layout)?;
            let element = input
                .subrange::<Inner::Wire>(offset)
                .map_err(<Self::Owner as OwnerAdapter>::access_layout)?;
            Inner::prove(Self::position(index, j), element)?;
        }
        Ok(())
    }

    fn read<'wire>(
        _: usize,
        input: SharedInput<'wire, Self::Wire>,
    ) -> Result<Self::Read<'wire>, <Self::Owner as OwnerAdapter>::AccessError> {
        ArrayRef::<'wire, Inner::Read<'wire>, N, Inner>::prove(input)
    }

    fn make_mut<'wire>(
        index: usize,
        input: ExclusiveInput<'wire, Self::Wire>,
        token: Self::Token,
    ) -> Result<Self::Mut<'wire>, <Self::Owner as OwnerAdapter>::AccessError> {
        ArrayMut::<'wire, Inner::Read<'wire>, N, Inner>::prove_at(
            input,
            token,
            index.saturating_mul(N),
        )
    }

    fn preflight<'wire, 'value>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError> {
        for (j, element_value) in value.iter().enumerate() {
            let offset = checked_element_offset(j, Inner::STRIDE)
                .map_err(<Self::Owner as OwnerAdapter>::mutation_layout)?;
            let element = input
                .subrange::<Inner::Wire>(offset)
                .map_err(<Self::Owner as OwnerAdapter>::mutation_layout)?;
            Inner::preflight(Self::position(index, j), element, element_value)?;
        }
        Ok(())
    }

    fn commit<'wire, 'value>(
        index: usize,
        mut input: ExclusiveInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
        token: Self::Token,
    ) {
        for (j, element_value) in value.iter().enumerate() {
            let offset = match checked_element_offset(j, Inner::STRIDE) {
                Ok(offset) => offset,
                Err(_) => unreachable!("a preflighted nested row retains every element offset"),
            };
            let element = match input.subrange_mut::<Inner::Wire>(offset) {
                Ok(element) => element,
                Err(_) => unreachable!("a preflighted nested row retains every element range"),
            };
            Inner::commit(Self::position(index, j), element, element_value, token);
        }
    }

    fn preflight_init<'wire, 'value>(
        index: usize,
        input: SharedInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
    ) -> Result<(), <Self::Owner as OwnerAdapter>::MutationError> {
        for (j, element_value) in value.iter().enumerate() {
            let offset = checked_element_offset(j, Inner::STRIDE)
                .map_err(<Self::Owner as OwnerAdapter>::mutation_layout)?;
            let element = input
                .subrange::<Inner::Wire>(offset)
                .map_err(<Self::Owner as OwnerAdapter>::mutation_layout)?;
            Inner::preflight_init(Self::position(index, j), element, element_value)?;
        }
        Ok(())
    }

    fn commit_init<'wire, 'value>(
        index: usize,
        mut input: ExclusiveInput<'wire, Self::Wire>,
        value: &Self::Value<'value>,
        token: Self::Token,
    ) {
        for (j, element_value) in value.iter().enumerate() {
            let offset = match checked_element_offset(j, Inner::STRIDE) {
                Ok(offset) => offset,
                Err(_) => unreachable!("a preflighted nested row retains every element offset"),
            };
            let element = match input.subrange_mut::<Inner::Wire>(offset) {
                Ok(element) => element,
                Err(_) => unreachable!("a preflighted nested row retains every element range"),
            };
            Inner::commit_init(Self::position(index, j), element, element_value, token);
        }
    }

    fn index_error(index: usize, len: usize) -> <Self::Owner as OwnerAdapter>::MutationError {
        Inner::index_error(index, len)
    }

    fn length_error(
        actual: usize,
        expected: usize,
    ) -> <Self::Owner as OwnerAdapter>::MutationError {
        Inner::length_error(actual, expected)
    }
}

/// Computes one element's byte range, rejecting multiplication/addition overflow.
#[doc(hidden)]
#[inline]
pub fn checked_element_range(
    available: usize,
    index: usize,
    stride: usize,
    element_size: usize,
) -> Result<Range<usize>, LayoutError> {
    let offset = index
        .checked_mul(stride)
        .ok_or(LayoutError::OffsetOverflow)?;
    checked_range(available, offset, element_size)
}

/// Computes one element offset without deciding whether `index` is logical bounds-valid.
///
/// Generated array adapters use this after their own logical index check, allowing
/// them to map an out-of-bounds index to the owning schema error type.
#[doc(hidden)]
#[inline]
pub fn checked_element_offset(index: usize, stride: usize) -> Result<usize, LayoutError> {
    index.checked_mul(stride).ok_or(LayoutError::OffsetOverflow)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use core::{
        fmt,
        mem::size_of,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use std::{sync::Mutex, vec::Vec};

    use super::*;
    use crate::{
        __private::{ArrayElementAdapter, InputAccess, OwnerAdapter, SharedInput},
        error::{ErrorKind, ErrorPathSegment, SchemaError},
    };

    fn release<T>(value: T) {
        drop(value);
    }

    #[derive(Debug, Eq, PartialEq)]
    struct TestError;

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test error")
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Layout
        }

        fn schema(&self) -> &'static str {
            "ArrayTest"
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

    static NEXT_VALIDATED_INDEX: AtomicUsize = AtomicUsize::new(0);
    static BYTE_ADAPTER_INSTRUMENTATION: Mutex<()> = Mutex::new(());

    struct ByteAdapter;

    #[derive(Clone, Copy)]
    struct ByteToken;
    impl InputAccess for ByteAdapter {
        type Token = ByteToken;
    }

    struct ByteElementMut<'wire>(ExclusiveInput<'wire, u8>, ByteToken);

    impl ByteElementMut<'_> {
        fn get(&self) -> u8 {
            self.0.read_copy::<u8>(0).expect("exact byte element")
        }

        fn set(&mut self, value: u8) {
            self.0
                .subrange_bytes_mut::<ByteAdapter>(0, 1, self.1)
                .unwrap()[0] = value;
        }
    }

    static NEXT_PREFLIGHT_INDEX: AtomicUsize = AtomicUsize::new(0);
    static NEXT_COMMITTED_INDEX: AtomicUsize = AtomicUsize::new(0);

    impl ArrayElementAdapter for ByteAdapter {
        type Wire = u8;
        type ArrayWire<const N: usize> = [u8; N];
        type Owner = TestOwner;
        type Read<'wire> = u8;
        type Value<'source> = u8;
        type Mut<'wire> = ByteElementMut<'wire>;

        const STRIDE: usize = 1;

        fn prove<'wire>(
            index: usize,
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<(), TestError> {
            assert_eq!(index, NEXT_VALIDATED_INDEX.fetch_add(1, Ordering::SeqCst));
            assert_eq!(
                input.read_copy::<u8>(0).expect("exact byte element"),
                index as u8
            );
            Ok(())
        }

        fn read<'wire>(
            _: usize,
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<Self::Read<'wire>, TestError> {
            Ok(input.read_copy::<u8>(0).expect("exact byte element"))
        }

        fn make_mut<'wire>(
            _: usize,
            input: ExclusiveInput<'wire, Self::Wire>,
            token: Self::Token,
        ) -> Result<Self::Mut<'wire>, TestError> {
            Ok(ByteElementMut(input, token))
        }

        fn preflight<'wire, 'value>(
            index: usize,
            _: SharedInput<'wire, Self::Wire>,
            value: &Self::Value<'value>,
        ) -> Result<(), TestError> {
            assert_eq!(index, NEXT_PREFLIGHT_INDEX.fetch_add(1, Ordering::SeqCst));
            if *value == 9 {
                Err(TestError)
            } else {
                assert!(index < 3);
                Ok(())
            }
        }

        fn commit<'wire, 'value>(
            index: usize,
            mut input: ExclusiveInput<'wire, Self::Wire>,
            value: &Self::Value<'value>,
            token: Self::Token,
        ) {
            assert_eq!(index, NEXT_COMMITTED_INDEX.fetch_add(1, Ordering::SeqCst));
            input
                .subrange_bytes_mut::<ByteAdapter>(0, 1, token)
                .unwrap()[0] = *value;
        }

        fn index_error(_: usize, _: usize) -> TestError {
            TestError
        }

        fn length_error(_: usize, _: usize) -> TestError {
            TestError
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Borrowed<'wire>(&'wire u8);

    struct BorrowedAdapter;

    #[derive(Clone, Copy)]
    struct BorrowedToken;
    impl InputAccess for BorrowedAdapter {
        type Token = BorrowedToken;
    }

    impl ArrayElementAdapter for BorrowedAdapter {
        type Wire = u8;
        type ArrayWire<const N: usize> = [u8; N];
        type Owner = TestOwner;
        type Read<'wire> = Borrowed<'wire>;
        type Value<'source> = u8;
        type Mut<'wire> = ();

        const STRIDE: usize = 1;

        fn prove<'wire>(_: usize, _: SharedInput<'wire, Self::Wire>) -> Result<(), TestError> {
            Ok(())
        }

        fn read<'wire>(
            _: usize,
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<Self::Read<'wire>, TestError> {
            let bytes = input
                .subrange_bytes::<BorrowedAdapter>(0, 1, BorrowedToken)
                .expect("exact byte element");
            Ok(Borrowed(&bytes[0]))
        }

        fn make_mut<'wire>(
            _: usize,
            _: ExclusiveInput<'wire, Self::Wire>,
            _: Self::Token,
        ) -> Result<Self::Mut<'wire>, TestError> {
            Ok(())
        }

        fn preflight<'wire, 'value>(
            _: usize,
            _: SharedInput<'wire, Self::Wire>,
            _: &Self::Value<'value>,
        ) -> Result<(), TestError> {
            Ok(())
        }

        fn commit<'wire, 'value>(
            _: usize,
            _: ExclusiveInput<'wire, Self::Wire>,
            _: &Self::Value<'value>,
            _: Self::Token,
        ) {
        }

        fn index_error(_: usize, _: usize) -> TestError {
            TestError
        }

        fn length_error(_: usize, _: usize) -> TestError {
            TestError
        }
    }

    #[test]
    fn array_proof_reads_and_iteration_are_ordered_and_compact() {
        let _instrumentation = BYTE_ADAPTER_INSTRUMENTATION
            .lock()
            .expect("array instrumentation lock");
        let bytes = [0_u8, 1, 2];
        let input = SharedInput::<[u8; 3]>::from_checked(&bytes).unwrap();
        NEXT_VALIDATED_INDEX.store(0, Ordering::SeqCst);
        let view = ArrayRef::<u8, 3, ByteAdapter>::prove(input).unwrap();

        assert_eq!(NEXT_VALIDATED_INDEX.load(Ordering::SeqCst), 3);
        assert_eq!(view.get(0), Some(0));
        assert_eq!(view.get(2), Some(2));
        assert_eq!(view.get(3), None);
        assert_eq!(view.copy_into(), [0, 1, 2]);
        assert_eq!(
            size_of::<ArrayRef<'_, u8, 3, ByteAdapter>>(),
            size_of::<SharedInput<'_, [u8; 3]>>()
        );

        let mut iterator = view.iter();
        assert_eq!(iterator.len(), 3);
        assert_eq!(iterator.next(), Some(0));
        assert_eq!(iterator.len(), 2);
        assert_eq!(iterator.next(), Some(1));
        assert_eq!(iterator.next(), Some(2));
        assert_eq!(iterator.next(), None);
    }

    #[test]
    fn array_materialization_rebinds_borrowed_logical_elements() {
        let bytes = [9_u8, 8];
        let input = SharedInput::<[u8; 2]>::from_checked(&bytes).unwrap();
        let view = ArrayRef::<Borrowed<'_>, 2, BorrowedAdapter>::prove(input).unwrap();
        let logical = view.copy_into();

        assert_eq!(*logical[0].0, 9);
        assert_eq!(*logical[1].0, 8);
    }

    #[test]
    fn element_range_checks_multiplication_before_bounds() {
        assert_eq!(
            checked_element_range(1, usize::MAX, 2, 1),
            Err(LayoutError::OffsetOverflow)
        );
    }

    #[test]
    fn element_range_uses_wire_stride() {
        assert_eq!(checked_element_range(12, 2, 4, 3).unwrap(), 8..11);
    }

    #[test]
    fn mutable_array_preflights_entire_source_then_commits_in_order() {
        let _instrumentation = BYTE_ADAPTER_INSTRUMENTATION
            .lock()
            .expect("array instrumentation lock");
        let mut bytes = [0_u8, 1, 2];
        NEXT_VALIDATED_INDEX.store(0, Ordering::SeqCst);
        let input = ExclusiveInput::<[u8; 3]>::from_checked(&mut bytes).unwrap();
        let mut array = ArrayMut::<u8, 3, ByteAdapter>::prove(input, ByteToken).unwrap();
        assert_eq!(NEXT_VALIDATED_INDEX.load(Ordering::SeqCst), 3);

        assert_eq!(array.get(1), Some(1));
        assert_eq!(array.get(3), None);
        assert!(array.get_mut(3).is_none());
        {
            let mut element = array.get_mut(1).unwrap();
            assert_eq!(element.get(), 1);
            element.set(6);
        }
        assert_eq!(array.copy_into(), [0, 6, 2]);
        let mut iterator = array.iter();
        assert_eq!(iterator.next(), Some(0));
        assert_eq!(iterator.next(), Some(6));
        assert_eq!(iterator.next(), Some(2));
        assert_eq!(iterator.next(), None);

        assert_eq!(array.set(4, 7), Err(TestError));
        NEXT_PREFLIGHT_INDEX.store(0, Ordering::SeqCst);
        NEXT_COMMITTED_INDEX.store(0, Ordering::SeqCst);
        assert_eq!(array.copy_from(&[7, 9, 8]), Err(TestError));
        assert_eq!(NEXT_PREFLIGHT_INDEX.load(Ordering::SeqCst), 2);
        assert_eq!(NEXT_COMMITTED_INDEX.load(Ordering::SeqCst), 0);
        assert_eq!(array.copy_into(), [0, 6, 2]);
        assert_eq!(array.copy_from(&[7, 8]), Err(TestError));
        assert_eq!(array.copy_into(), [0, 6, 2]);

        NEXT_PREFLIGHT_INDEX.store(0, Ordering::SeqCst);
        NEXT_COMMITTED_INDEX.store(0, Ordering::SeqCst);
        array.copy_from(&[3, 4, 5]).unwrap();
        assert_eq!(NEXT_PREFLIGHT_INDEX.load(Ordering::SeqCst), 3);
        assert_eq!(NEXT_COMMITTED_INDEX.load(Ordering::SeqCst), 3);
        assert_eq!(array.copy_into(), [3, 4, 5]);
        release(array);
        assert_eq!(bytes, [3, 4, 5]);
    }

    #[derive(Debug, Eq, PartialEq)]
    struct PositionError(usize);

    impl fmt::Display for PositionError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "position error {}", self.0)
        }
    }

    impl core::error::Error for PositionError {}

    impl SchemaError for PositionError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Layout
        }

        fn schema(&self) -> &'static str {
            "PositionTest"
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            Some(ErrorPathSegment::Index(self.0))
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }

        fn __fmt_leaf(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("position error")
        }
    }

    struct PositionOwner;

    impl OwnerAdapter for PositionOwner {
        type AccessError = PositionError;
        type MutationError = PositionError;

        fn access_layout(_: LayoutError) -> Self::AccessError {
            PositionError(usize::MAX)
        }

        fn mutation_layout(_: LayoutError) -> Self::MutationError {
            PositionError(usize::MAX)
        }
    }

    static POSITION_INSTRUMENTATION: Mutex<Vec<(&'static str, usize)>> = Mutex::new(Vec::new());

    fn trace(step: &'static str, position: usize) {
        POSITION_INSTRUMENTATION
            .lock()
            .expect("position trace lock")
            .push((step, position));
    }

    fn take_trace() -> Vec<(&'static str, usize)> {
        std::mem::take(
            &mut *POSITION_INSTRUMENTATION
                .lock()
                .expect("position trace lock"),
        )
    }

    /// Leaf adapter that records the flat position every failing-capable
    /// operation receives and rejects the byte `0xff` at that position.
    struct PositionAdapter;

    #[derive(Clone, Copy)]
    struct PositionToken;

    impl InputAccess for PositionAdapter {
        type Token = PositionToken;
    }

    impl ArrayElementAdapter for PositionAdapter {
        type Wire = u8;
        type ArrayWire<const N: usize> = [u8; N];
        type Owner = PositionOwner;
        type Read<'wire> = u8;
        type Value<'source> = u8;
        type Mut<'wire> = ();

        const STRIDE: usize = 1;

        fn prove<'wire>(
            index: usize,
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<(), PositionError> {
            trace("prove", index);
            match input.read_copy::<u8>(0).expect("exact byte element") {
                0xff => Err(PositionError(index)),
                _ => Ok(()),
            }
        }

        fn read<'wire>(
            _: usize,
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<Self::Read<'wire>, PositionError> {
            Ok(input.read_copy::<u8>(0).expect("exact byte element"))
        }

        fn make_mut<'wire>(
            _: usize,
            _: ExclusiveInput<'wire, Self::Wire>,
            _: Self::Token,
        ) -> Result<Self::Mut<'wire>, PositionError> {
            Ok(())
        }

        fn preflight<'wire, 'value>(
            index: usize,
            _: SharedInput<'wire, Self::Wire>,
            value: &Self::Value<'value>,
        ) -> Result<(), PositionError> {
            trace("preflight", index);
            match *value {
                0xff => Err(PositionError(index)),
                _ => Ok(()),
            }
        }

        fn commit<'wire, 'value>(
            index: usize,
            mut input: ExclusiveInput<'wire, Self::Wire>,
            value: &Self::Value<'value>,
            token: Self::Token,
        ) {
            trace("commit", index);
            input
                .subrange_bytes_mut::<PositionAdapter>(0, 1, token)
                .unwrap()[0] = *value;
        }

        fn index_error(index: usize, _: usize) -> PositionError {
            PositionError(index)
        }

        fn length_error(actual: usize, _: usize) -> PositionError {
            PositionError(actual)
        }
    }

    type PositionRows = NestedArrayAdapter<PositionAdapter, 3>;
    type PositionRowView<'wire> =
        ArrayRef<'wire, ArrayRef<'wire, u8, 3, PositionAdapter>, 2, PositionRows>;

    #[test]
    fn nested_array_proof_visits_flat_positions_in_order_and_reads_row_views() {
        let _serial = NESTED_SERIAL.lock().expect("nested serialization lock");
        take_trace();
        let bytes = [10_u8, 11, 12, 13, 14, 15];
        let input = SharedInput::<[[u8; 3]; 2]>::from_checked(&bytes).unwrap();
        let view = PositionRowView::prove(input).unwrap();
        // The combinator is stateless: both views stay input-sized.
        assert_eq!(
            size_of::<PositionRowView<'_>>(),
            size_of::<SharedInput<'_, [[u8; 3]; 2]>>()
        );
        assert_eq!(
            take_trace(),
            (0..6)
                .map(|position| ("prove", position))
                .collect::<Vec<_>>()
        );

        let row = view.get(1).unwrap();
        assert_eq!(row.get(2), Some(15));
        assert_eq!(row.get(3), None);
        assert_eq!(row.copy_into(), [13, 14, 15]);
        assert_eq!(view.get(2).map(|_| ()), None);
        assert_eq!(
            view.iter().map(|row| row.copy_into()).collect::<Vec<_>>(),
            [[10, 11, 12], [13, 14, 15]]
        );
        // Re-forming a row view is the only trace a read leaves.
        take_trace();

        // The first failing element is reported by its flat position and stops
        // the walk, so an enclosing path can be reconstructed from it.
        let bad = [10_u8, 11, 12, 13, 0xff, 15];
        let input = SharedInput::<[[u8; 3]; 2]>::from_checked(&bad).unwrap();
        let error = PositionRowView::prove(input).err().unwrap();
        assert_eq!(error, PositionError(4));
        assert_eq!(
            take_trace(),
            (0..5)
                .map(|position| ("prove", position))
                .collect::<Vec<_>>()
        );
    }

    static NESTED_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn nested_mutable_rows_know_their_flat_origin_and_two_pass_atomicity_spans_rows() {
        let _serial = NESTED_SERIAL.lock().expect("nested serialization lock");
        let mut bytes = [1_u8, 2, 3, 4, 5, 6];
        let input = ExclusiveInput::<[[u8; 3]; 2]>::from_checked(&mut bytes).unwrap();
        let mut rows = ArrayMut::<(), 2, PositionRows>::prove(input, PositionToken).unwrap();
        take_trace();

        // A row handle reports positions in the enclosing array's flat order.
        {
            let mut row = rows.get_mut(1).unwrap();
            assert_eq!(take_trace(), [("prove", 3), ("prove", 4), ("prove", 5)]);
            row.set(1, 9).unwrap();
            assert_eq!(take_trace(), [("preflight", 4), ("commit", 4)]);
            assert_eq!(row.set(1, 0xff), Err(PositionError(4)));
            assert_eq!(row.set(3, 1), Err(PositionError(3)));
            take_trace();
            row.copy_from(&[7, 8, 9]).unwrap();
            assert_eq!(
                take_trace(),
                [
                    ("preflight", 3),
                    ("preflight", 4),
                    ("preflight", 5),
                    ("commit", 3),
                    ("commit", 4),
                    ("commit", 5)
                ]
            );
        }
        assert!(rows.get_mut(2).is_none());

        // A whole-row replacement preflights every row element before committing.
        rows.set(0, [20, 21, 22]).unwrap();
        assert_eq!(
            take_trace(),
            [
                ("preflight", 0),
                ("preflight", 1),
                ("preflight", 2),
                ("commit", 0),
                ("commit", 1),
                ("commit", 2)
            ]
        );

        // A late invalid element in a later row leaves every row unchanged.
        assert_eq!(
            rows.copy_from(&[[1, 2, 3], [4, 0xff, 6]]),
            Err(PositionError(4))
        );
        assert_eq!(
            take_trace(),
            (0..5)
                .map(|position| ("preflight", position))
                .collect::<Vec<_>>()
        );
        assert_eq!(rows.set(0, [0xff, 0, 0]), Err(PositionError(0)));
        assert_eq!(rows.set(2, [0, 0, 0]), Err(PositionError(2)));
        assert_eq!(rows.copy_from(&[[0, 0, 0]]), Err(PositionError(1)));
        take_trace();
        release(rows);
        assert_eq!(bytes, [20, 21, 22, 7, 8, 9]);
    }
}
