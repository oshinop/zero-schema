//! Allocation-free constrained field mutation capabilities.
//!
//! Every public handle owns only a short exclusive reborrow of one already
//! proved field. Its generated adapter maps all failures back to the concrete
//! owning schema mutation error; neither a handle nor an adapter exposes a raw
//! mutable wire view to callers.

use core::{marker::PhantomData, ops::Range};

use crate::{
    __private::{
        ExclusiveInput, FixedBytesMutationAdapter, OptionFieldAdapter, OwnerAdapter,
        ScalarMutationAdapter, StringMutationAdapter,
    },
    error::LayoutError,
};

/// Computes an in-bounds range after checking addition for overflow.
#[doc(hidden)]
#[inline]
pub fn checked_range(
    available: usize,
    offset: usize,
    length: usize,
) -> Result<Range<usize>, LayoutError> {
    let end = offset
        .checked_add(length)
        .ok_or(LayoutError::OffsetOverflow)?;
    if end > available {
        return Err(LayoutError::InsufficientBytes {
            required: end,
            actual: available,
        });
    }
    Ok(offset..end)
}

/// Selects a bounded mutable subrange without clearing or initializing it.
#[doc(hidden)]
#[inline]
pub fn subrange_mut(
    destination: &mut [u8],
    offset: usize,
    length: usize,
) -> Result<&mut [u8], LayoutError> {
    let range = checked_range(destination.len(), offset, length)?;
    Ok(&mut destination[range])
}

/// Copies `source` into one bounded destination range.
#[doc(hidden)]
#[inline]
pub fn copy_bytes_at(
    destination: &mut [u8],
    offset: usize,
    source: &[u8],
) -> Result<(), LayoutError> {
    let range = checked_range(destination.len(), offset, source.len())?;
    destination[range].copy_from_slice(source);
    Ok(())
}

/// Copies a whole source only when the destination has exactly the same length.
#[doc(hidden)]
#[inline]
pub fn copy_exact(destination: &mut [u8], source: &[u8]) -> Result<(), LayoutError> {
    if destination.len() != source.len() {
        return Err(LayoutError::IncorrectSize {
            expected: destination.len(),
            actual: source.len(),
        });
    }
    destination.copy_from_slice(source);
    Ok(())
}

/// A short exclusive mutation capability for one scalar, Boolean, or scalar enum.
///
/// Generated accessors supply an owner-specific adapter. `set` completes source
/// validation before the generated adapter commits the selected field range.
pub struct ScalarMut<'view, LogicalT, Adapter>
where
    Adapter: ScalarMutationAdapter<Logical = LogicalT>,
{
    input: ExclusiveInput<'view, Adapter::Wire>,
    token: Adapter::Token,
    _adapter: PhantomData<fn() -> (LogicalT, Adapter)>,
}

impl<'view, LogicalT, Adapter> ScalarMut<'view, LogicalT, Adapter>
where
    LogicalT: Copy,
    Adapter: ScalarMutationAdapter<Logical = LogicalT>,
{
    /// Validates the exact current field bytes before constructing a mutation
    /// capability. Layout-only exclusive inputs cannot skip this proof.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: ExclusiveInput<'view, Adapter::Wire>,
        token: Adapter::Token,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        Adapter::read(input.shared())?;
        Ok(Self {
            input,
            token,
            _adapter: PhantomData,
        })
    }

    /// Returns the current logical scalar through a shared short reborrow.
    #[inline]
    pub fn get(&self) -> LogicalT {
        match Adapter::read(self.input.shared()) {
            Ok(value) => value,
            Err(_) => unreachable!("a checked scalar handle preserves field validity"),
        }
    }

    /// Replaces the scalar after complete source preflight.
    #[inline]
    pub fn set(
        &mut self,
        value: LogicalT,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        Adapter::preflight(value)?;
        Adapter::commit(self.input.reborrow(), value, self.token);
        Ok(())
    }
}

/// A short exclusive mutation capability for one bounded string field.
///
/// `Adapter::Logical` is one of `str`, `CStr`, `U16Str`, or `U16CStr`. The
/// preflight observes capacity and prefix form before `commit` copies an active
/// value and deliberately leaves unused capacity unchanged.
pub struct StringMut<'view, Adapter>
where
    Adapter: StringMutationAdapter,
{
    input: ExclusiveInput<'view, Adapter::Wire>,
    token: Adapter::Token,
    _adapter: PhantomData<fn() -> Adapter>,
}

impl<'view, Adapter> StringMut<'view, Adapter>
where
    Adapter: StringMutationAdapter,
{
    /// Validates the exact current field bytes before constructing a mutation
    /// capability. Layout-only exclusive inputs cannot skip this proof.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: ExclusiveInput<'view, Adapter::Wire>,
        token: Adapter::Token,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        Adapter::read(input.shared())?;
        Ok(Self {
            input,
            token,
            _adapter: PhantomData,
        })
    }

    /// Returns the active logical string through a shared short reborrow.
    #[inline]
    pub fn get(&self) -> &Adapter::Logical {
        match Adapter::read(self.input.shared()) {
            Ok(value) => value,
            Err(_) => unreachable!("a checked string handle preserves field validity"),
        }
    }

    /// Replaces the active string without clearing unused capacity.
    #[inline]
    pub fn set(
        &mut self,
        value: &Adapter::Logical,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        Adapter::preflight(self.input.shared(), value)?;
        Adapter::commit(self.input.reborrow(), value, self.token);
        Ok(())
    }
}

/// A short exclusive mutation capability for one fixed byte field.
///
/// `set` accepts an exact logical byte sequence only. The generated adapter
/// reports source-length failure in the enclosing schema's mutation error type
/// before it writes the selected byte field.
pub struct BytesMut<'view, Adapter>
where
    Adapter: FixedBytesMutationAdapter,
{
    input: ExclusiveInput<'view, Adapter::Wire>,
    token: Adapter::Token,
    _adapter: PhantomData<fn() -> Adapter>,
}

impl<'view, Adapter> BytesMut<'view, Adapter>
where
    Adapter: FixedBytesMutationAdapter,
{
    /// Validates the exact current field bytes before constructing a mutation
    /// capability. Layout-only exclusive inputs cannot skip this proof.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: ExclusiveInput<'view, Adapter::Wire>,
        token: Adapter::Token,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        Adapter::read(input.shared())?;
        Ok(Self {
            input,
            token,
            _adapter: PhantomData,
        })
    }

    /// Returns the current fixed byte value through a shared short reborrow.
    #[inline]
    pub fn get(&self) -> &[u8] {
        match Adapter::read(self.input.shared()) {
            Ok(value) => value,
            Err(_) => unreachable!("a checked byte handle preserves field validity"),
        }
    }

    /// Replaces exactly this field's bytes after full source preflight.
    #[inline]
    pub fn set(
        &mut self,
        value: &[u8],
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        Adapter::preflight(value)?;
        Adapter::commit(self.input.reborrow(), value, self.token);
        Ok(())
    }
}

/// An O(1)-state exclusive capability for one zero-sentinel optional field.
///
/// Presence is never cached: each accessor scans the complete declared storage
/// span. A nonzero span is then validated through the inner value adapter.
pub struct OptionMut<'view, LogicalT, Adapter>
where
    Adapter: OptionFieldAdapter,
{
    input: ExclusiveInput<'view, Adapter::StorageWire>,
    token: Adapter::Token,
    _adapter: PhantomData<fn() -> (LogicalT, Adapter)>,
}

impl<'view, LogicalT, Adapter> OptionMut<'view, LogicalT, Adapter>
where
    Adapter: OptionFieldAdapter,
{
    /// Validates a nonzero optional field before constructing its mutation
    /// capability. An all-zero complete storage span is valid absence.
    #[doc(hidden)]
    #[inline]
    pub fn prove(
        input: ExclusiveInput<'view, Adapter::StorageWire>,
        token: Adapter::Token,
    ) -> Result<Self, <Adapter::Owner as OwnerAdapter>::AccessError> {
        if !input.shared().is_all_zero() {
            let value = input
                .subrange::<Adapter::ValueWire>(Adapter::VALUE_OFFSET)
                .map_err(<Adapter::Owner as OwnerAdapter>::access_layout)?;
            Adapter::validate_present(value)?;
        }

        Ok(Self {
            input,
            token,
            _adapter: PhantomData,
        })
    }

    /// Returns the current optional observation through a short shared
    /// reborrow. The live complete storage span is rescanned every call.
    #[inline]
    pub fn get(&self) -> Option<Adapter::Read<'_>> {
        if self.input.shared().is_all_zero() {
            return None;
        }

        let value = match self
            .input
            .subrange::<Adapter::ValueWire>(Adapter::VALUE_OFFSET)
        {
            Ok(value) => value,
            Err(_) => unreachable!("a proved optional handle retains its value subrange"),
        };
        match Adapter::read_present(value) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved optional handle retains a valid present value"),
        }
    }

    /// Returns a field-local mutable child capability when the live storage is
    /// present. The returned short exclusive borrow prevents `set` until it is
    /// released.
    #[inline]
    pub fn get_mut(&mut self) -> Option<Adapter::Mut<'_>> {
        if self.input.shared().is_all_zero() {
            return None;
        }

        let token = self.token;
        let value = match self
            .input
            .subrange_mut::<Adapter::ValueWire>(Adapter::VALUE_OFFSET)
        {
            Ok(value) => value,
            Err(_) => unreachable!("a proved optional handle retains its value subrange"),
        };
        match Adapter::make_present_mut(value, token) {
            Ok(value) => Some(value),
            Err(_) => unreachable!("a proved optional handle retains a valid present value"),
        }
    }

    /// Replaces this optional field after all source-dependent work completes.
    /// `None` clears only the complete declared storage span. `Some` leaves
    /// field-local padding untouched and initializes only the value wire.
    #[inline]
    pub fn set<'source>(
        &mut self,
        value: Option<Adapter::Value<'source>>,
    ) -> Result<(), <Adapter::Owner as OwnerAdapter>::MutationError> {
        match value {
            None => Adapter::clear(self.input.reborrow(), self.token),
            Some(value) => {
                let input = self
                    .input
                    .subrange::<Adapter::ValueWire>(Adapter::VALUE_OFFSET)
                    .map_err(<Adapter::Owner as OwnerAdapter>::mutation_layout)?;
                Adapter::preflight_init(input, &value)?;

                let input = self
                    .input
                    .subrange_mut::<Adapter::ValueWire>(Adapter::VALUE_OFFSET)
                    .map_err(<Adapter::Owner as OwnerAdapter>::mutation_layout)?;
                Adapter::commit_init(input, &value, self.token);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::{fmt, mem::size_of};

    use super::*;
    use crate::{
        __private::{
            FixedBytesMutationAdapter, InputAccess, OptionFieldAdapter, OwnerAdapter,
            ScalarMutationAdapter, SharedInput, StringMutationAdapter,
        },
        error::{ErrorKind, ErrorPathSegment, SchemaError},
        strings::{commit_str, preflight_str, prove_str},
        wire::{StrWire, U8},
    };

    fn release<T>(value: T) {
        drop(value);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestError {
        Rejected,
        Capacity,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test mutation error")
        }
    }

    impl core::error::Error for TestError {}

    impl SchemaError for TestError {
        fn kind(&self) -> ErrorKind {
            match self {
                Self::Rejected => ErrorKind::UnknownEnumValue,
                Self::Capacity => ErrorKind::CapacityExceeded,
            }
        }

        fn schema(&self) -> &'static str {
            "MutationTest"
        }

        fn segment(&self) -> Option<ErrorPathSegment> {
            None
        }

        fn child(&self) -> Option<&dyn SchemaError> {
            None
        }

        fn __fmt_leaf(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test mutation error")
        }
    }

    struct TestOwner;

    impl OwnerAdapter for TestOwner {
        type AccessError = TestError;
        type MutationError = TestError;

        fn access_layout(_: LayoutError) -> Self::AccessError {
            TestError::Rejected
        }

        fn mutation_layout(_: LayoutError) -> Self::MutationError {
            TestError::Rejected
        }
    }

    struct ByteScalar;

    #[derive(Clone, Copy)]
    struct ByteScalarToken;
    impl InputAccess for ByteScalar {
        type Token = ByteScalarToken;
    }

    impl ScalarMutationAdapter for ByteScalar {
        type Wire = u8;
        type Owner = TestOwner;
        type Logical = u8;

        fn read(input: SharedInput<'_, Self::Wire>) -> Result<Self::Logical, TestError> {
            input.read_copy::<u8>(0).map_err(|_| TestError::Rejected)
        }

        fn preflight(value: Self::Logical) -> Result<(), TestError> {
            if value == 9 {
                Err(TestError::Rejected)
            } else {
                Ok(())
            }
        }

        fn commit(
            mut input: ExclusiveInput<'_, Self::Wire>,
            value: Self::Logical,
            token: Self::Token,
        ) {
            input.subrange_bytes_mut::<Self>(0, 1, token).unwrap()[0] = value;
        }
    }

    struct ByteOption;

    #[derive(Clone, Copy)]
    struct ByteOptionToken;

    impl InputAccess for ByteOption {
        type Token = ByteOptionToken;
    }

    struct ByteOptionMut<'wire> {
        input: ExclusiveInput<'wire, u8>,
        token: ByteOptionToken,
    }

    impl ByteOptionMut<'_> {
        fn set(&mut self, value: u8) {
            self.input
                .subrange_bytes_mut::<ByteOption>(0, 1, self.token)
                .unwrap()[0] = value;
        }
    }

    impl OptionFieldAdapter for ByteOption {
        type StorageWire = [u8; 4];
        type ValueWire = u8;
        type Owner = TestOwner;
        type Read<'wire> = &'wire u8;
        type Value<'source> = u8;
        type Mut<'wire> = ByteOptionMut<'wire>;

        const VALUE_OFFSET: usize = 1;

        fn validate_present(input: SharedInput<'_, Self::ValueWire>) -> Result<(), TestError> {
            match input.read_copy::<u8>(0).map_err(|_| TestError::Rejected)? {
                0 => Err(TestError::Rejected),
                _ => Ok(()),
            }
        }

        fn read_present<'wire>(
            input: SharedInput<'wire, Self::ValueWire>,
        ) -> Result<Self::Read<'wire>, TestError> {
            Self::validate_present(input)?;
            Ok(&input.subrange_bytes::<Self>(0, 1, ByteOptionToken).unwrap()[0])
        }

        fn make_present_mut<'wire>(
            input: ExclusiveInput<'wire, Self::ValueWire>,
            token: Self::Token,
        ) -> Result<Self::Mut<'wire>, TestError> {
            Self::validate_present(input.shared())?;
            Ok(ByteOptionMut { input, token })
        }

        fn preflight_init<'wire, 'source>(
            _: SharedInput<'wire, Self::ValueWire>,
            value: &Self::Value<'source>,
        ) -> Result<(), TestError> {
            if *value == 0 || *value == 9 {
                Err(TestError::Rejected)
            } else {
                Ok(())
            }
        }

        fn commit_init<'wire, 'source>(
            mut input: ExclusiveInput<'wire, Self::ValueWire>,
            value: &Self::Value<'source>,
            token: Self::Token,
        ) {
            input.subrange_bytes_mut::<Self>(0, 1, token).unwrap()[0] = *value;
        }
    }

    struct NarrowString;
    #[derive(Clone, Copy)]
    struct NarrowStringToken;
    impl InputAccess for NarrowString {
        type Token = NarrowStringToken;
    }

    impl StringMutationAdapter for NarrowString {
        type Wire = StrWire<U8, 4>;
        type Owner = TestOwner;
        type Logical = str;

        fn read<'wire>(
            input: SharedInput<'wire, Self::Wire>,
        ) -> Result<&'wire Self::Logical, TestError> {
            let length = input
                .read_copy::<U8>(StrWire::<U8, 4>::LEN_OFFSET)
                .map_err(|_| TestError::Rejected)?;
            let data = input
                .subrange_bytes::<Self>(StrWire::<U8, 4>::DATA_OFFSET, 4, NarrowStringToken)
                .map_err(|_| TestError::Rejected)?;
            prove_str(&length, data).map_err(|_| TestError::Rejected)
        }

        fn preflight(
            input: SharedInput<'_, Self::Wire>,
            value: &Self::Logical,
        ) -> Result<(), TestError> {
            let prefix = input
                .subrange_bytes::<Self>(0, StrWire::<U8, 4>::DATA_OFFSET, NarrowStringToken)
                .unwrap();
            let data = input
                .subrange_bytes::<Self>(StrWire::<U8, 4>::DATA_OFFSET, 4, NarrowStringToken)
                .unwrap();
            preflight_str::<U8>(prefix, data, value).map_err(|_| TestError::Capacity)
        }

        fn commit(
            mut input: ExclusiveInput<'_, Self::Wire>,
            value: &Self::Logical,
            token: Self::Token,
        ) {
            let bytes = input
                .subrange_bytes_mut::<Self>(0, size_of::<Self::Wire>(), token)
                .unwrap();
            let (prefix, data) = bytes.split_at_mut(StrWire::<U8, 4>::DATA_OFFSET);
            commit_str::<U8>(prefix, data, value);
        }
    }

    struct FixedThree;
    #[derive(Clone, Copy)]
    struct FixedThreeToken;
    impl InputAccess for FixedThree {
        type Token = FixedThreeToken;
    }

    impl FixedBytesMutationAdapter for FixedThree {
        type Wire = [u8; 3];
        type Owner = TestOwner;

        fn read(input: SharedInput<'_, Self::Wire>) -> Result<&[u8], TestError> {
            input
                .subrange_bytes::<Self>(0, size_of::<Self::Wire>(), FixedThreeToken)
                .map_err(|_| TestError::Rejected)
        }

        fn preflight(value: &[u8]) -> Result<(), TestError> {
            if value.len() == 3 {
                Ok(())
            } else {
                Err(TestError::Capacity)
            }
        }

        fn commit(mut input: ExclusiveInput<'_, Self::Wire>, value: &[u8], token: Self::Token) {
            input
                .subrange_bytes_mut::<Self>(0, size_of::<Self::Wire>(), token)
                .unwrap()
                .copy_from_slice(value);
        }
    }

    #[test]
    fn scalar_handle_uses_short_reborrows_and_preserves_failed_source() {
        let mut bytes = [4_u8];
        let input = ExclusiveInput::<u8>::from_checked(&mut bytes).unwrap();
        let mut handle = ScalarMut::<u8, ByteScalar>::prove(input, ByteScalarToken).unwrap();

        assert_eq!(handle.get(), 4);
        assert_eq!(handle.set(9), Err(TestError::Rejected));
        assert_eq!(handle.get(), 4);
        handle.set(7).unwrap();
        assert_eq!(handle.get(), 7);
        release(handle);
        assert_eq!(bytes, [7]);
    }

    #[test]
    fn bounded_string_and_fixed_bytes_preflight_before_selected_writes() {
        let mut string_bytes = [2_u8, b'h', b'i', 0xee, 0xee];
        let input = ExclusiveInput::<StrWire<U8, 4>>::from_checked(&mut string_bytes).unwrap();
        let mut string = StringMut::<NarrowString>::prove(input, NarrowStringToken).unwrap();
        assert_eq!(string.get(), "hi");
        string.set("ok").unwrap();
        assert_eq!(string.get(), "ok");
        assert_eq!(string.set("too long"), Err(TestError::Capacity));
        release(string);
        assert_eq!(string_bytes, [2, b'o', b'k', 0xee, 0xee]);

        let mut fixed_bytes = [0xa5, 0xa5, 0xa5];
        let input = ExclusiveInput::<[u8; 3]>::from_checked(&mut fixed_bytes).unwrap();
        let mut fixed = BytesMut::<FixedThree>::prove(input, FixedThreeToken).unwrap();
        assert_eq!(fixed.set(&[1, 2]), Err(TestError::Capacity));
        assert_eq!(fixed.get(), &[0xa5; 3]);
        fixed.set(&[1, 2, 3]).unwrap();
        release(fixed);
        assert_eq!(fixed_bytes, [1, 2, 3]);
    }

    #[test]
    fn option_handle_rescans_exact_storage_and_uses_short_live_borrows() {
        let mut storage = [0x91_u8, 0xa5, 1, 0xd4, 0xfe, 0x71];
        let input = ExclusiveInput::<[u8; 4]>::from_checked(&mut storage[1..5]).unwrap();
        let mut option = OptionMut::<u8, ByteOption>::prove(input, ByteOptionToken).unwrap();

        let value = option.get().unwrap();
        assert_eq!(*value, 1);
        release(value);

        assert_eq!(option.set(Some(9)), Err(TestError::Rejected));
        assert_eq!(*option.get().unwrap(), 1);

        let mut child = option.get_mut().unwrap();
        child.set(2);
        release(child);
        assert_eq!(*option.get().unwrap(), 2);

        option.set(Some(7)).unwrap();
        assert_eq!(*option.get().unwrap(), 7);
        release(option);
        assert_eq!(storage, [0x91, 0xa5, 7, 0xd4, 0xfe, 0x71]);

        let input = ExclusiveInput::<[u8; 4]>::from_checked(&mut storage[1..5]).unwrap();
        let mut option = OptionMut::<u8, ByteOption>::prove(input, ByteOptionToken).unwrap();
        option.set(None).unwrap();
        assert!(option.get().is_none());
        release(option);
        assert_eq!(storage, [0x91, 0, 0, 0, 0, 0x71]);

        let input = ExclusiveInput::<[u8; 4]>::from_checked(&mut storage[1..5]).unwrap();
        let mut option = OptionMut::<u8, ByteOption>::prove(input, ByteOptionToken).unwrap();
        option.set(Some(7)).unwrap();
        release(option);
        assert_eq!(storage, [0x91, 0, 7, 0, 0, 0x71]);

        storage[1..5].copy_from_slice(&[0xa5, 0, 0, 0]);
        let input = ExclusiveInput::<[u8; 4]>::from_checked(&mut storage[1..5]).unwrap();
        assert!(matches!(
            OptionMut::<u8, ByteOption>::prove(input, ByteOptionToken),
            Err(TestError::Rejected)
        ));
        assert_eq!(storage, [0x91, 0xa5, 0, 0, 0, 0x71]);
    }
}
