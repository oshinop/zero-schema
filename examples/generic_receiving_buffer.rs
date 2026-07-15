use core::mem::{align_of_val, size_of_val};

use zero_schema::{FieldKind, TypeKind, zero};

#[zero]
#[derive(Debug, PartialEq)]
struct Bytes<'a, const N: usize> {
    bytes: &'a [u8; N],
}

type FourBytes = Bytes<'static, 4>;

// Reviewed producer output for the fully concrete `Bytes<'static, 4>` root.
const REVIEWED_PRODUCER_BYTES: [u8; 4] = [0x10, 0x20, 0x30, 0x40];

const _: [(); 4] = [(); FourBytes::SCHEMA_SIZE];
const _: [(); 1] = [(); FourBytes::SCHEMA_ALIGN];
const _: [(); 4] = [(); FourBytes::SCHEMA_STRIDE];

#[repr(C, align(1))]
struct ProducerBytes {
    bytes: [u8; FourBytes::SCHEMA_SIZE],
}

impl ProducerBytes {
    const fn reviewed() -> Self {
        Self {
            bytes: REVIEWED_PRODUCER_BYTES,
        }
    }
}

fn main() {
    let mut storage = zero_schema::schema_buffer!(Bytes<'static, 4>);
    assert_eq!(
        (
            FourBytes::SCHEMA_SIZE,
            FourBytes::SCHEMA_ALIGN,
            FourBytes::SCHEMA_STRIDE,
        ),
        (4, 1, 4)
    );
    assert_eq!(
        (
            storage.as_bytes().len(),
            size_of_val(&storage),
            align_of_val(&storage),
        ),
        (
            FourBytes::SCHEMA_SIZE,
            FourBytes::SCHEMA_STRIDE,
            FourBytes::SCHEMA_ALIGN,
        )
    );
    assert_eq!(
        storage
            .as_bytes()
            .as_ptr()
            .align_offset(FourBytes::SCHEMA_ALIGN),
        0
    );

    let layout = FourBytes::LAYOUT;
    assert_eq!(
        (
            layout.name(),
            layout.kind(),
            layout.size(),
            layout.align(),
            layout.stride(),
        ),
        ("Bytes", TypeKind::Struct, 4, 1, 4)
    );
    assert_eq!(layout.fields().len(), 1);
    let field = layout.fields()[0];
    assert_eq!(
        (
            field.declaration_index(),
            field.name(),
            field.offset(),
            field.size(),
            field.align(),
            field.is_optional(),
        ),
        (0, "bytes", 0, 4, 1, false)
    );
    let FieldKind::FixedBytes { length } = field.kind() else {
        panic!("bytes must expose fixed-byte metadata");
    };
    assert_eq!(length, 4);

    // `schema_buffer!` initializes aligned Rust receiving storage only. It does
    // not construct a generic `Bytes<'static, 4>` value from schema data.
    assert_eq!(storage.as_bytes(), &[0; FourBytes::SCHEMA_SIZE]);

    // Producer output is independently reviewed bytes; copy it before access.
    let producer = ProducerBytes::reviewed();
    storage.as_bytes_mut().copy_from_slice(&producer.bytes);
    assert_eq!(storage.as_bytes(), &producer.bytes);

    let view = FourBytes::access(storage.as_bytes()).expect("producer-filled storage is valid");
    let borrowed_bytes = view.bytes();
    assert_eq!(borrowed_bytes, &REVIEWED_PRODUCER_BYTES);
    println!(
        "generic receiving buffer has {} producer bytes",
        borrowed_bytes.len()
    );
}
