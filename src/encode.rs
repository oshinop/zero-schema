use crate::error::LayoutError;

#[cfg(test)]
extern crate std;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InstrumentationCounts {
    root_fills: usize,
    root_zeroed_bytes: usize,
    copied_bytes: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Default)]
struct InstrumentationState {
    enabled: bool,
    counts: InstrumentationCounts,
}

#[cfg(test)]
std::thread_local! {
    static INSTRUMENTATION: core::cell::Cell<InstrumentationState> =
        const { core::cell::Cell::new(InstrumentationState {
            enabled: false,
            counts: InstrumentationCounts { root_fills: 0, root_zeroed_bytes: 0, copied_bytes: 0 },
        }) };
}

#[cfg(test)]
fn record_root_fill(bytes: usize) {
    INSTRUMENTATION.with(|cell| {
        let mut state = cell.get();
        if state.enabled {
            state.counts.root_fills += 1;
            state.counts.root_zeroed_bytes += bytes;
            cell.set(state);
        }
    });
}

#[cfg(test)]
fn record_content_copy(bytes: usize) {
    INSTRUMENTATION.with(|cell| {
        let mut state = cell.get();
        if state.enabled {
            state.counts.copied_bytes += bytes;
            cell.set(state);
        }
    });
}

#[cfg(test)]
struct InstrumentationGuard {
    previous: InstrumentationState,
}

#[cfg(test)]
impl InstrumentationGuard {
    fn enable_reset() -> Self {
        let previous = INSTRUMENTATION.with(|cell| {
            let previous = cell.get();
            cell.set(InstrumentationState {
                enabled: true,
                counts: InstrumentationCounts::default(),
            });
            previous
        });
        Self { previous }
    }

    fn reset(&self) {
        INSTRUMENTATION.with(|cell| {
            let mut state = cell.get();
            state.counts = InstrumentationCounts::default();
            cell.set(state);
        });
    }

    fn read(&self) -> InstrumentationCounts {
        INSTRUMENTATION.with(|cell| cell.get().counts)
    }
}

#[cfg(test)]
impl Drop for InstrumentationGuard {
    fn drop(&mut self) {
        INSTRUMENTATION.with(|cell| cell.set(self.previous));
    }
}

/// Initialized byte storage whose address satisfies the alignment of `W`.
///
/// The zero-length `_align` field carries alignment without storing a `W` or
/// requiring uninitialized memory. The public byte view is always exactly
/// `N` bytes long; the value itself may include trailing padding so that its
/// size is a multiple of `align_of::<W>()`.
///
/// `W` must be the wire type for a fully concrete schema. Prefer
/// [`crate::make_buffer_for!`] to construct schema buffers without spelling the
/// wire projection or byte length.
#[repr(C)]
pub struct AlignedBytes<W, const N: usize> {
    _align: [W; 0],
    bytes: [u8; N],
}

impl<W, const N: usize> AlignedBytes<W, N> {
    /// Creates an initialized buffer containing exactly `N` zero bytes.
    pub const fn zeroed() -> Self {
        Self {
            _align: [],
            bytes: [0; N],
        }
    }

    /// Returns the initialized `N`-byte wire storage.
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns mutable access to the initialized `N`-byte wire storage.
    pub const fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl<W, const N: usize> AsRef<[u8]> for AlignedBytes<W, N> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<W, const N: usize> AsMut<[u8]> for AlignedBytes<W, N> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

/// A bounded writer over storage that was zeroed at the root.
///
/// Subranges preserve the root's zeroed state and therefore never clear their
/// contents again.
pub struct Prezeroed<'dst> {
    bytes: &'dst mut [u8],
}

impl<'dst> Prezeroed<'dst> {
    /// Zeros an entire destination exactly once and creates its root writer.
    #[doc(hidden)]
    pub fn new(root: &'dst mut [u8]) -> Self {
        root.fill(0);
        #[cfg(test)]
        record_root_fill(root.len());
        Self { bytes: root }
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn write(&mut self, offset: usize, source: &[u8]) -> Result<(), LayoutError> {
        let end = offset
            .checked_add(source.len())
            .ok_or(LayoutError::OffsetOverflow)?;
        if end > self.bytes.len() {
            return Err(LayoutError::InsufficientBytes {
                required: end,
                actual: self.bytes.len(),
            });
        }
        self.bytes[offset..end].copy_from_slice(source);
        #[cfg(test)]
        record_content_copy(source.len());
        Ok(())
    }

    pub fn subrange(&mut self, offset: usize, length: usize) -> Result<Prezeroed<'_>, LayoutError> {
        let end = offset
            .checked_add(length)
            .ok_or(LayoutError::OffsetOverflow)?;
        if end > self.bytes.len() {
            return Err(LayoutError::InsufficientBytes {
                required: end,
                actual: self.bytes.len(),
            });
        }
        Ok(Prezeroed {
            bytes: &mut self.bytes[offset..end],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(16))]
    struct Align16;

    #[test]
    fn aligned_bytes_are_initialized_and_exact_length() {
        let buffer = AlignedBytes::<Align16, 7>::zeroed();
        assert_eq!(buffer.as_bytes(), &[0; 7]);
        assert_eq!(buffer.as_bytes().len(), 7);
    }

    #[test]
    fn aligned_bytes_carry_wire_alignment_and_stride_size() {
        let buffer = AlignedBytes::<Align16, 7>::zeroed();
        assert_eq!(core::mem::align_of_val(&buffer), 16);
        assert_eq!(core::mem::size_of_val(&buffer), 16);
        assert_eq!((buffer.as_bytes().as_ptr() as usize) % 16, 0);
    }

    #[test]
    fn aligned_bytes_mutation_and_slice_traits_share_storage() {
        let mut buffer = AlignedBytes::<u32, 5>::zeroed();
        buffer.as_bytes_mut()[1] = 3;
        AsMut::<[u8]>::as_mut(&mut buffer)[4] = 9;
        assert_eq!(AsRef::<[u8]>::as_ref(&buffer), &[0, 3, 0, 0, 9]);
    }

    #[test]
    fn root_fills_once_and_subranges_do_not_refill() {
        let mut bytes = [0xa5; 8];
        {
            let mut root = Prezeroed::new(&mut bytes);
            assert_eq!(root.len(), 8);
            root.write(1, &[1, 2, 3]).unwrap();
            {
                let mut child = root.subrange(2, 3).unwrap();
                child.write(1, &[9]).unwrap();
            }
            root.write(7, &[7]).unwrap();
        }
        assert_eq!(bytes, [0, 1, 2, 9, 0, 0, 0, 7]);
    }

    #[test]
    fn writes_and_subranges_are_confined() {
        let mut bytes = [0xff; 10];
        {
            let mut root = Prezeroed::new(&mut bytes[2..8]);
            assert_eq!(
                root.write(5, &[1, 2]).unwrap_err(),
                LayoutError::InsufficientBytes {
                    required: 7,
                    actual: 6,
                }
            );
            assert_eq!(
                root.subrange(5, 2).err().unwrap(),
                LayoutError::InsufficientBytes {
                    required: 7,
                    actual: 6,
                }
            );
        }
        assert_eq!(bytes, [0xff, 0xff, 0, 0, 0, 0, 0, 0, 0xff, 0xff]);
    }

    #[test]
    fn overflow_precedes_bounds() {
        let mut bytes = [1; 1];
        let mut root = Prezeroed::new(&mut bytes);
        assert_eq!(
            root.write(usize::MAX, &[1, 2]).unwrap_err(),
            LayoutError::OffsetOverflow
        );
        assert_eq!(
            root.subrange(usize::MAX, 2).err().unwrap(),
            LayoutError::OffsetOverflow
        );
    }

    #[derive(crate::ZeroSchema)]
    #[repr(u8)]
    enum InstrumentedTag {
        Leaf = 7,
    }

    #[derive(crate::ZeroSchema)]
    struct InstrumentedLeaf {
        value: u32,
    }

    #[derive(crate::ZeroSchema)]
    #[zero(tag = InstrumentedTag)]
    enum InstrumentedPayload {
        #[zero(tag = InstrumentedTag::Leaf)]
        Leaf(InstrumentedLeaf),
    }

    #[derive(crate::ZeroSchema)]
    struct InstrumentedRoot<'a> {
        scalar: u16,
        #[zero(capacity = 5, len_type = u8)]
        text: &'a str,
        fixed: &'a [u8; 3],
        payload: InstrumentedPayload,
    }

    #[test]
    fn generated_nested_encoding_counts_one_fill_and_active_copies() {
        let mut disabled = [0xa5; 2];
        Prezeroed::new(&mut disabled).write(0, &[7]).unwrap();

        let fixed = [0x31, 0x32, 0x33];
        let value = InstrumentedRoot {
            scalar: 0x2211,
            text: "abc",
            fixed: &fixed,
            payload: InstrumentedPayload::Leaf(InstrumentedLeaf { value: 0x5241_4030 }),
        };
        let mut buffer = crate::make_buffer_for!(InstrumentedRoot<'static>);
        let guard = InstrumentationGuard::enable_reset();
        value.encode_into(buffer.as_bytes_mut()).unwrap();

        // Independent wire-active accounting: root scalar, UTF-8 length prefix and
        // logical bytes, fixed bytes, union tag, and selected leaf scalar.
        let active_logical_bytes = 2 + 1 + value.text.len() + fixed.len() + 1 + 4;
        assert_eq!(
            guard.read(),
            InstrumentationCounts {
                root_fills: 1,
                root_zeroed_bytes: InstrumentedRoot::WIRE_SIZE,
                copied_bytes: active_logical_bytes,
            }
        );

        guard.reset();
        assert_eq!(guard.read(), InstrumentationCounts::default());
        drop(guard);

        let mut disabled_again = [0xa5; 1];
        Prezeroed::new(&mut disabled_again).write(0, &[9]).unwrap();
        let verification = InstrumentationGuard::enable_reset();
        assert_eq!(verification.read(), InstrumentationCounts::default());
    }

    #[test]
    fn instrumentation_guard_restores_state_during_unwind() {
        let result = std::panic::catch_unwind(|| {
            let _guard = InstrumentationGuard::enable_reset();
            let mut bytes = [1; 1];
            let _root = Prezeroed::new(&mut bytes);
            panic!("exercise RAII cleanup");
        });
        assert!(result.is_err());

        let mut bytes = [1; 1];
        let _root = Prezeroed::new(&mut bytes);
        let verification = InstrumentationGuard::enable_reset();
        assert_eq!(verification.read(), InstrumentationCounts::default());
    }
}
