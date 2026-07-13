use core::mem;

use zerocopy::{FromBytes, Immutable, KnownLayout};

use crate::error::LayoutError;

/// A typed wire view paired with the exact bytes from which it was decoded.
///
/// Keeping the original bytes is necessary for inspecting padding without ever
/// treating an aggregate wire value as bytes.
pub struct DecodeInput<'src, W> {
    wire: &'src W,
    bytes: &'src [u8],
}
impl<W> Copy for DecodeInput<'_, W> {}

impl<W> Clone for DecodeInput<'_, W> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'src, W> DecodeInput<'src, W>
where
    W: FromBytes + KnownLayout + Immutable,
{
    /// Constructs an input whose byte range is exactly one `W`.
    #[doc(hidden)]
    pub fn from_exact(bytes: &'src [u8]) -> Result<Self, LayoutError> {
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
            address: bytes.as_ptr() as usize,
        })?;
        Ok(Self { wire, bytes })
    }

    /// Constructs an input from the first complete `W` in `bytes`.
    #[doc(hidden)]
    pub fn from_prefix(bytes: &'src [u8]) -> Result<Self, LayoutError> {
        let required = mem::size_of::<W>();
        if bytes.len() < required {
            return Err(LayoutError::InsufficientBytes {
                required,
                actual: bytes.len(),
            });
        }
        check_alignment::<W>(bytes)?;

        let accepted = &bytes[..required];
        let wire = W::ref_from_bytes(accepted).map_err(|_| LayoutError::Misaligned {
            required: mem::align_of::<W>(),
            address: accepted.as_ptr() as usize,
        })?;
        Ok(Self {
            wire,
            bytes: accepted,
        })
    }

    pub const fn wire(&self) -> &'src W {
        self.wire
    }

    pub const fn bytes(&self) -> &'src [u8] {
        self.bytes
    }

    pub fn subrange<F>(&self, offset: usize) -> Result<DecodeInput<'src, F>, LayoutError>
    where
        F: FromBytes + KnownLayout + Immutable,
    {
        let required = mem::size_of::<F>();
        let end = offset
            .checked_add(required)
            .ok_or(LayoutError::OffsetOverflow)?;
        if end > self.bytes.len() {
            return Err(LayoutError::InsufficientBytes {
                required: end,
                actual: self.bytes.len(),
            });
        }

        DecodeInput::from_exact(&self.bytes[offset..end]).map_err(|error| match error {
            LayoutError::Misaligned { required, address } => {
                LayoutError::Misaligned { required, address }
            }
            _ => LayoutError::InsufficientBytes {
                required: end,
                actual: self.bytes.len(),
            },
        })
    }

    #[doc(hidden)]
    pub const fn shorten<'short>(&self) -> DecodeInput<'short, W>
    where
        'src: 'short,
    {
        DecodeInput {
            wire: self.wire,
            bytes: self.bytes,
        }
    }
}

fn check_alignment<W>(bytes: &[u8]) -> Result<(), LayoutError> {
    let required = mem::align_of::<W>();
    let address = bytes.as_ptr() as usize;
    if address % required != 0 {
        return Err(LayoutError::Misaligned { required, address });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(8))]
    struct Aligned([u8; 24]);

    #[test]
    fn length_errors_precede_alignment() {
        let storage = Aligned([0; 24]);
        let misaligned = &storage.0[1..];
        assert_eq!(
            DecodeInput::<u64>::from_exact(&misaligned[..7])
                .err()
                .unwrap(),
            LayoutError::IncorrectSize {
                expected: 8,
                actual: 7,
            }
        );
        assert_eq!(
            DecodeInput::<u64>::from_prefix(&misaligned[..7])
                .err()
                .unwrap(),
            LayoutError::InsufficientBytes {
                required: 8,
                actual: 7,
            }
        );
        assert!(matches!(
            DecodeInput::<u64>::from_exact(&misaligned[..8]),
            Err(LayoutError::Misaligned { required: 8, .. })
        ));
    }

    #[test]
    fn size_only_errors_on_aligned_storage() {
        let storage = Aligned([0; 24]);
        assert_eq!(
            DecodeInput::<u64>::from_exact(&storage.0[..9])
                .err()
                .unwrap(),
            LayoutError::IncorrectSize {
                expected: 8,
                actual: 9,
            }
        );
    }

    #[test]
    fn subrange_overflow_precedes_bounds_and_input_is_reusable() {
        let storage = Aligned([0; 24]);
        let input = DecodeInput::<u64>::from_exact(&storage.0[..8]).unwrap();
        assert_eq!(
            input.subrange::<u64>(usize::MAX).err().unwrap(),
            LayoutError::OffsetOverflow
        );

        let first = input.subrange::<u32>(0).unwrap();
        let second = input.subrange::<u32>(4).unwrap();
        assert_eq!(first.bytes().len(), 4);
        assert_eq!(second.bytes().len(), 4);
    }

    #[test]
    fn prefix_retains_exact_wire_bytes_and_leaves_reusable_remainder() {
        let storage = Aligned([0; 24]);
        let source = &storage.0[..16];
        let input = DecodeInput::<u64>::from_prefix(source).unwrap();
        assert_eq!(input.bytes(), &source[..8]);
        assert_eq!(&source[input.bytes().len()..], &source[8..]);
    }

    #[test]
    fn subrange_reports_bounds_before_alignment_and_misalignment_control() {
        let storage = Aligned([0; 24]);
        let input = DecodeInput::<u64>::from_exact(&storage.0[..8]).unwrap();
        assert_eq!(
            input.subrange::<u64>(4).err().unwrap(),
            LayoutError::InsufficientBytes {
                required: 12,
                actual: 8,
            }
        );
        assert!(matches!(
            input.subrange::<u32>(1),
            Err(LayoutError::Misaligned { required: 4, .. })
        ));
    }

    #[test]
    fn exact_origin_bytes_include_padding() {
        #[repr(C)]
        #[derive(zerocopy::FromBytes, zerocopy::KnownLayout, zerocopy::Immutable)]
        struct Padded {
            byte: u8,
            word: u32,
        }

        let mut storage = Aligned([0; 24]);
        storage.0[..8].copy_from_slice(&[1, 0xa1, 0xa2, 0xa3, 2, 0, 0, 0]);
        let input = DecodeInput::<Padded>::from_exact(&storage.0[..8]).unwrap();
        assert_eq!(input.bytes(), &[1, 0xa1, 0xa2, 0xa3, 2, 0, 0, 0]);
    }

    #[test]
    fn copy_does_not_require_copy_wire() {
        fn copy_input<'a, W>(input: DecodeInput<'a, W>) -> DecodeInput<'a, W> {
            let copied = input;
            let _also_copied = input;
            copied
        }

        struct NotCopy;
        let value = NotCopy;
        let input = DecodeInput {
            wire: &value,
            bytes: &[],
        };
        let _ = copy_input(input);
    }
}
