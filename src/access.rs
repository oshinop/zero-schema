//! Checked byte inputs used internally by generated access capabilities.
//!
//! These types retain the original, exact byte span. They deliberately do not
//! decode logical values or expose the complete backing allocation.

use core::{marker::PhantomData, mem};

use zerocopy::{FromBytes, Immutable, KnownLayout};

use crate::{
    __private::{InputAccess, RootInputAccess},
    error::LayoutError,
    mutation::checked_range,
};

/// Initialized, correctly aligned receiving storage for producer-owned wire bytes.
///
/// `W` supplies alignment only; it is an opaque generated wire projection rather
/// than a value stored in this buffer. `N` is the root schema's exact byte size.
/// Construction fills the byte array with zeroes solely to initialize Rust memory.
/// Those bytes have no schema interpretation: callers must let the root's
/// `access` or `access_mut` operation establish type validity after a producer has
/// populated the storage.
#[repr(C)]
pub struct SchemaBuffer<W, const N: usize> {
    _align: [W; 0],
    bytes: [u8; N],
}

impl<W, const N: usize> SchemaBuffer<W, N> {
    /// Creates initialized receiving storage with no implied schema validity.
    #[inline]
    pub const fn new() -> Self {
        Self {
            _align: [],
            bytes: [0; N],
        }
    }

    /// Returns exactly the root's receiving byte span.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns exactly the root's mutable receiving byte span.
    ///
    /// Mutating these initialized bytes does not establish schema validity.
    #[inline]
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl<W, const N: usize> Default for SchemaBuffer<W, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Exact, checked shared input for one all-bit-valid wire value.
#[doc(hidden)]
pub struct SharedInput<'bytes, W> {
    bytes: &'bytes [u8],
    wire: &'bytes W,
}

impl<W> Copy for SharedInput<'_, W> {}

impl<W> Clone for SharedInput<'_, W> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'bytes, W> SharedInput<'bytes, W>
where
    W: FromBytes + KnownLayout + Immutable,
{
    /// Runtime-only exact constructor used after a checked root or field range
    /// has already established its bounds and alignment.
    #[inline]
    pub(crate) fn from_checked(bytes: &'bytes [u8]) -> Result<Self, LayoutError> {
        let expected = mem::size_of::<W>();
        if bytes.len() != expected {
            return Err(LayoutError::IncorrectSize {
                expected,
                actual: bytes.len(),
            });
        }
        check_alignment::<W>(bytes)?;

        let wire = W::ref_from_bytes(bytes).map_err(|_| LayoutError::Misaligned {
            required: mem::align_of::<W>(),
            address: bytes.as_ptr().addr(),
        })?;
        Ok(Self { bytes, wire })
    }

    /// Tests whether every byte in this exact checked span is zero.
    ///
    /// This deliberately exposes no byte slice. Optional fields use it only
    /// over their complete declared storage wire, including local padding.
    #[doc(hidden)]
    #[inline]
    pub fn is_all_zero(&self) -> bool {
        self.bytes.iter().all(|byte| *byte == 0)
    }

    /// Reads one Copy wire leaf at a checked field offset. Unlike the removed
    /// aggregate `wire` view, this never exposes a reference to `W`.
    #[doc(hidden)]
    #[inline]
    pub fn read_copy<F>(&self, offset: usize) -> Result<F, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable + Copy,
    {
        let selected = self.subrange::<F>(offset)?;
        Ok(*selected.wire)
    }

    /// Selects another exact, checked wire subrange.
    #[doc(hidden)]
    #[inline]
    pub fn subrange<F>(&self, offset: usize) -> Result<SharedInput<'bytes, F>, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable,
    {
        let range = checked_range(self.bytes.len(), offset, mem::size_of::<F>())?;
        SharedInput::from_checked(&self.bytes[range])
    }

    /// Returns a bounded byte subrange for generated field support only.
    #[doc(hidden)]
    #[inline]
    pub fn subrange_bytes<B: InputAccess>(
        &self,
        offset: usize,
        length: usize,
        _: B::Token,
    ) -> Result<&'bytes [u8], LayoutError> {
        let range = checked_range(self.bytes.len(), offset, length)?;
        Ok(&self.bytes[range])
    }

    /// Shortens the capability lifetime without changing the checked span.
    #[doc(hidden)]
    #[inline]
    pub const fn shorten<'short>(&self) -> SharedInput<'short, W>
    where
        'bytes: 'short,
    {
        SharedInput {
            bytes: self.bytes,
            wire: self.wire,
        }
    }
}

impl<'bytes, W> SharedInput<'bytes, W>
where
    W: RootInputAccess + FromBytes + KnownLayout + Immutable,
{
    /// Checks one generated root wire span. Its private generated token prevents
    /// arbitrary initialized bytes from becoming a schema input.
    #[doc(hidden)]
    #[inline]
    pub fn from_exact(bytes: &'bytes [u8], _: W::Token) -> Result<Self, LayoutError> {
        Self::from_checked(bytes)
    }
}

/// Exact, checked exclusive input for one all-bit-valid wire value.
///
/// The mutable slice remains the authority for storage. Immutable typed views
/// are re-formed only for short shared borrows, so no mutable aggregate wire
/// reference or `IntoBytes` bound is needed.
#[doc(hidden)]
pub struct ExclusiveInput<'bytes, W> {
    bytes: &'bytes mut [u8],
    marker: PhantomData<W>,
}

impl<'bytes, W> ExclusiveInput<'bytes, W>
where
    W: FromBytes + KnownLayout + Immutable,
{
    /// Runtime-only exact constructor used after a checked root or field range
    /// has already established its bounds and alignment.
    #[inline]
    pub(crate) fn from_checked(bytes: &'bytes mut [u8]) -> Result<Self, LayoutError> {
        let expected = mem::size_of::<W>();
        if bytes.len() != expected {
            return Err(LayoutError::IncorrectSize {
                expected,
                actual: bytes.len(),
            });
        }
        check_alignment::<W>(bytes)?;
        W::ref_from_bytes(&*bytes).map_err(|_| LayoutError::Misaligned {
            required: mem::align_of::<W>(),
            address: bytes.as_ptr().addr(),
        })?;
        Ok(Self {
            bytes,
            marker: PhantomData,
        })
    }

    /// Borrows the stable checked root as a shared capability.
    #[doc(hidden)]
    #[inline]
    pub fn shared(&self) -> SharedInput<'_, W> {
        shared_exact::<W>(&*self.bytes)
    }

    /// Reads one Copy wire leaf at a checked field offset without exposing an
    /// aggregate wire view.
    #[doc(hidden)]
    #[inline]
    pub fn read_copy<F>(&self, offset: usize) -> Result<F, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable + Copy,
    {
        self.shared().read_copy(offset)
    }

    /// Selects a short shared typed field subrange.
    #[doc(hidden)]
    #[inline]
    pub fn subrange<F>(&self, offset: usize) -> Result<SharedInput<'_, F>, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable,
    {
        self.shared().subrange(offset)
    }

    /// Selects a short exclusive typed field subrange.
    #[doc(hidden)]
    #[inline]
    pub fn subrange_mut<F>(&mut self, offset: usize) -> Result<ExclusiveInput<'_, F>, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable,
    {
        let range = checked_range(self.bytes.len(), offset, mem::size_of::<F>())?;
        ExclusiveInput::from_checked(&mut self.bytes[range])
    }

    /// Returns a bounded shared field slice for generated support only.
    #[doc(hidden)]
    #[inline]
    pub fn subrange_bytes<B: InputAccess>(
        &self,
        offset: usize,
        length: usize,
        _: B::Token,
    ) -> Result<&[u8], LayoutError> {
        let range = checked_range(self.bytes.len(), offset, length)?;
        Ok(&self.bytes[range])
    }

    /// Returns a bounded exclusive field slice for generated support only.
    #[doc(hidden)]
    #[inline]
    pub fn subrange_bytes_mut<B: InputAccess>(
        &mut self,
        offset: usize,
        length: usize,
        _: B::Token,
    ) -> Result<&mut [u8], LayoutError> {
        let range = checked_range(self.bytes.len(), offset, length)?;
        Ok(&mut self.bytes[range])
    }

    /// Clears this complete exact checked input while retaining a generated
    /// private token gate. It cannot select an arbitrary parent range.
    #[doc(hidden)]
    #[inline]
    pub fn clear_all<B: InputAccess>(&mut self, _: B::Token) {
        self.bytes.fill(0);
    }

    /// Reborrows the whole checked input exclusively for a shorter lifetime.
    #[doc(hidden)]
    #[inline]
    pub fn reborrow(&mut self) -> ExclusiveInput<'_, W> {
        exclusive_exact::<W>(&mut *self.bytes)
    }
}

impl<'bytes, W> ExclusiveInput<'bytes, W>
where
    W: RootInputAccess + FromBytes + KnownLayout + Immutable,
{
    /// Checks one generated root wire span. Its private generated token prevents
    /// arbitrary initialized bytes from becoming a schema input.
    #[doc(hidden)]
    #[inline]
    pub fn from_exact(bytes: &'bytes mut [u8], _: W::Token) -> Result<Self, LayoutError> {
        Self::from_checked(bytes)
    }
}

/// Exact shared schema proof branded by the generated support that validated it.
///
/// This token owns the same checked byte span used for validation. Its
/// constructor is runtime-private so layout-only [`SharedInput`] values cannot
/// be upgraded into a logical capability by downstream code.
#[doc(hidden)]
pub struct ProvedShared<'wire, Brand, W> {
    input: SharedInput<'wire, W>,
    _brand: PhantomData<fn() -> Brand>,
}

impl<Brand, W> Copy for ProvedShared<'_, Brand, W> {}

impl<Brand, W> Clone for ProvedShared<'_, Brand, W> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'wire, Brand, W> ProvedShared<'wire, Brand, W>
where
    Brand: InputAccess,
    W: FromBytes + KnownLayout + Immutable,
{
    #[inline]
    pub(crate) const fn new(input: SharedInput<'wire, W>) -> Self {
        Self {
            input,
            _brand: PhantomData,
        }
    }

    /// Consumes this evidence and returns its exact layout-only input.
    ///
    /// The returned input cannot mint a capability again without fresh support
    /// validation.
    #[doc(hidden)]
    #[inline]
    pub const fn into_input(self, _: Brand::Token) -> SharedInput<'wire, W> {
        self.input
    }

    /// Borrows the same proved span for a shorter capability lifetime.
    #[doc(hidden)]
    #[inline]
    pub const fn shorten<'short>(&self) -> ProvedShared<'short, Brand, W>
    where
        'wire: 'short,
    {
        ProvedShared {
            input: self.input.shorten(),
            _brand: PhantomData,
        }
    }
}

/// Exact exclusive schema proof branded by the generated support that
/// validated it.
///
/// It intentionally has no raw mutable reborrow: evidence may only be
/// consumed, shared, or reborrowed with the same brand and exact span.
#[doc(hidden)]
pub struct ProvedExclusive<'wire, Brand, W> {
    input: ExclusiveInput<'wire, W>,
    _brand: PhantomData<fn() -> Brand>,
}

impl<'wire, Brand, W> ProvedExclusive<'wire, Brand, W>
where
    Brand: InputAccess,
    W: FromBytes + KnownLayout + Immutable,
{
    #[inline]
    pub(crate) fn new(input: ExclusiveInput<'wire, W>) -> Self {
        Self {
            input,
            _brand: PhantomData,
        }
    }

    /// Reborrows the exact proved span as shared evidence with the same brand.
    #[doc(hidden)]
    #[inline]
    pub fn shared(&self) -> ProvedShared<'_, Brand, W> {
        ProvedShared::new(self.input.shared())
    }

    /// Reborrows the exact proved span exclusively with the same brand.
    #[doc(hidden)]
    #[inline]
    pub fn reborrow(&mut self) -> ProvedExclusive<'_, Brand, W> {
        ProvedExclusive::new(self.input.reborrow())
    }

    /// Consumes this evidence and returns its exact layout-only input.
    #[doc(hidden)]
    #[inline]
    pub fn into_input(self, _: Brand::Token) -> ExclusiveInput<'wire, W> {
        self.input
    }
}

#[inline]
fn check_alignment<W>(bytes: &[u8]) -> Result<(), LayoutError> {
    let required = mem::align_of::<W>();
    let address = bytes.as_ptr().addr();
    if address % required != 0 {
        return Err(LayoutError::Misaligned { required, address });
    }
    Ok(())
}

#[inline]
fn shared_exact<W>(bytes: &[u8]) -> SharedInput<'_, W>
where
    W: FromBytes + KnownLayout + Immutable,
{
    match SharedInput::from_checked(bytes) {
        Ok(input) => input,
        Err(_) => unreachable!("a checked input keeps its length and alignment"),
    }
}

#[inline]
fn exclusive_exact<W>(bytes: &mut [u8]) -> ExclusiveInput<'_, W>
where
    W: FromBytes + KnownLayout + Immutable,
{
    match ExclusiveInput::from_checked(bytes) {
        Ok(input) => input,
        Err(_) => unreachable!("a checked input keeps its length and alignment"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct TestToken;
    struct TestAccess;
    impl InputAccess for TestAccess {
        type Token = TestToken;
    }
    impl RootInputAccess for u64 {
        type Token = TestToken;
    }

    #[repr(align(8))]
    struct Aligned([u8; 24]);

    #[repr(align(16))]
    struct Align16;

    #[test]
    fn schema_buffer_is_exact_aligned_initialized_storage_only() {
        let mut buffer = SchemaBuffer::<Align16, 7>::new();

        assert_eq!(buffer.as_bytes(), &[0; 7]);
        assert_eq!(buffer.as_bytes().len(), 7);
        assert_eq!(core::mem::align_of_val(&buffer), 16);
        assert_eq!(core::mem::size_of_val(&buffer), 16);
        assert_eq!((buffer.as_bytes().as_ptr() as usize) % 16, 0);

        buffer.as_bytes_mut()[6] = 0xa5;
        assert_eq!(buffer.as_bytes(), &[0, 0, 0, 0, 0, 0, 0xa5]);
    }

    #[test]
    fn exact_size_error_precedes_alignment() {
        let storage = Aligned([0; 24]);
        let misaligned = &storage.0[1..];

        assert_eq!(
            SharedInput::<u64>::from_exact(&misaligned[..7], TestToken).err(),
            Some(LayoutError::IncorrectSize {
                expected: 8,
                actual: 7,
            })
        );
    }

    #[test]
    fn subrange_overflow_precedes_bounds() {
        let storage = Aligned([0; 24]);
        let input = SharedInput::<u64>::from_exact(&storage.0[..8], TestToken).unwrap();

        assert_eq!(
            input.subrange_bytes::<TestAccess>(usize::MAX, 1, TestToken),
            Err(LayoutError::OffsetOverflow)
        );
    }

    #[test]
    fn mutable_subranges_are_short_reborrows() {
        let mut storage = Aligned([0; 24]);
        let mut input = ExclusiveInput::<u64>::from_exact(&mut storage.0[..8], TestToken).unwrap();

        {
            let field = input
                .subrange_bytes_mut::<TestAccess>(2, 3, TestToken)
                .unwrap();
            field.copy_from_slice(&[7, 8, 9]);
        }

        assert_eq!(
            input.subrange_bytes::<TestAccess>(2, 3, TestToken).unwrap(),
            &[7, 8, 9]
        );
        assert_eq!(
            input.read_copy::<u64>(0).unwrap().to_ne_bytes()[2..5],
            [7, 8, 9]
        );
    }

    #[test]
    fn zero_scan_and_clear_cover_only_the_exact_checked_span() {
        let mut storage = [0x91_u8, 0, 0, 0, 0x71];
        {
            let input = SharedInput::<[u8; 3]>::from_checked(&storage[1..4]).unwrap();
            assert!(input.is_all_zero());
        }

        storage[2] = 0xa5;
        let input = SharedInput::<[u8; 3]>::from_checked(&storage[1..4]).unwrap();
        assert!(!input.is_all_zero());

        {
            let mut input = ExclusiveInput::<[u8; 3]>::from_checked(&mut storage[1..4]).unwrap();
            input.clear_all::<TestAccess>(TestToken);
        }
        assert_eq!(storage, [0x91, 0, 0, 0, 0x71]);
    }
}
