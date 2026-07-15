use core::mem::{align_of, size_of};

use zero_schema::{ArrayElementKind, ErrorKind, FieldKind, SchemaError, TypeKind, zero};

#[zero]
#[derive(Debug, PartialEq)]
struct Header {
    version: u8,
    active: bool,
}

#[zero(align = 4)]
#[derive(Debug, PartialEq)]
struct Record<'a> {
    sequence: u8,
    header: Header,
    samples: [u8; 3],
    token: &'a [u8; 4],
}

// Reviewed producer output for `Record`: Rust only receives and observes it.
const REVIEWED_PRODUCER_RECORD: [u8; 12] = [7, 2, 1, 3, 5, 8, 0x10, 0x20, 0x30, 0x40, 0xa1, 0xa2];

const _: [(); 12] = [(); Record::SCHEMA_SIZE];
const _: [(); 4] = [(); Record::SCHEMA_ALIGN];
const _: [(); 12] = [(); Record::SCHEMA_STRIDE];

#[repr(C, align(4))]
struct ProducerRecord {
    bytes: [u8; Record::SCHEMA_SIZE],
}

impl ProducerRecord {
    const fn reviewed() -> Self {
        Self {
            bytes: REVIEWED_PRODUCER_RECORD,
        }
    }
}

fn main() {
    let mut producer = ProducerRecord::reviewed();
    assert_eq!(size_of::<ProducerRecord>(), Record::SCHEMA_SIZE);
    assert_eq!(align_of::<ProducerRecord>(), Record::SCHEMA_ALIGN);

    let layout = Record::LAYOUT;
    assert_eq!(
        (
            layout.name(),
            layout.kind(),
            layout.size(),
            layout.align(),
            layout.stride(),
        ),
        ("Record", TypeKind::Struct, 12, 4, 12)
    );
    let fields = layout.fields();
    assert_eq!(fields.len(), 4);
    for (index, (name, offset, size, align)) in [
        ("sequence", 0, 1, 1),
        ("header", 1, 2, 1),
        ("samples", 3, 3, 1),
        ("token", 6, 4, 1),
    ]
    .iter()
    .enumerate()
    {
        let field = fields[index];
        assert_eq!(
            (
                field.declaration_index(),
                field.name(),
                field.offset(),
                field.size(),
                field.align(),
            ),
            (index, *name, *offset, *size, *align)
        );
        assert!(!field.is_optional());
    }
    let FieldKind::Schema { layout: header } = fields[1].kind() else {
        panic!("header metadata must describe Header");
    };
    assert_eq!(header.name(), "Header");
    let FieldKind::Array(samples) = fields[2].kind() else {
        panic!("samples metadata must describe a fixed array");
    };
    assert_eq!((samples.length(), samples.stride()), (3, 1));
    assert!(matches!(
        samples.element(),
        ArrayElementKind::Primitive { .. }
    ));
    assert_eq!(fields[3].kind(), FieldKind::FixedBytes { length: 4 });
    assert!(
        layout
            .padding()
            .iter()
            .any(|range| (range.start(), range.end()) == (10, 12))
    );
    assert_eq!(&producer.bytes[10..12], &[0xa1, 0xa2]);

    let view = Record::access(&producer.bytes).expect("reviewed producer bytes are valid");
    assert_eq!(view.sequence(), 7);
    assert_eq!((view.header().version(), view.header().active()), (2, true));
    let samples = view.samples();
    assert_eq!(samples.get(1), Some(5));
    assert_eq!(samples.get(3), None);
    assert!(samples.iter().eq([3, 5, 8]));
    assert_eq!(samples.copy_into(), [3, 5, 8]);
    assert_eq!(view.token(), &[0x10, 0x20, 0x30, 0x40]);

    // Materialization builds the logical record; ordinary wire padding has no field.
    let logical: Record<'_> = view.copy_into();
    assert_eq!(
        logical,
        Record {
            sequence: 7,
            header: Header {
                version: 2,
                active: true,
            },
            samples: [3, 5, 8],
            token: &[0x10, 0x20, 0x30, 0x40],
        }
    );

    {
        let mut record = Record::access_mut(&mut producer.bytes)
            .expect("the same producer bytes remain valid for constrained mutation");
        {
            let mut samples = record.samples_mut();
            assert_eq!(samples.get(0), Some(3));
            assert_eq!(samples.get(3), None);
            {
                let mut sample = samples.get_mut(1).expect("declared sample index");
                assert_eq!(sample.get(), 5);
                sample.set(13).expect("valid primitive replacement");
            }
            samples.set(0, 11).expect("valid primitive replacement");
            samples
                .copy_from(&[19, 21, 23])
                .expect("exact logical array copy is preflighted before writes");
            assert_eq!(samples.copy_into(), [19, 21, 23]);
        }
        record
            .copy_from(&RecordPatch {
                sequence: Some(9),
                header: Some(HeaderPatch {
                    version: None,
                    active: Some(false),
                }),
                ..Default::default()
            })
            .expect("the partial patch is fully preflighted before it writes");
    }

    let before_failed_array_copy = producer.bytes;
    {
        let mut record = Record::access_mut(&mut producer.bytes)
            .expect("the successful updates remain valid for another short reborrow");
        let mut samples = record.samples_mut();
        let error = samples
            .copy_from(&[31, 37])
            .expect_err("a fixed array copy requires exactly three elements");
        assert_eq!(error.kind(), ErrorKind::ArrayLengthMismatch);
    }
    assert_eq!(producer.bytes, before_failed_array_copy);
    assert_eq!(&producer.bytes[10..12], &[0xa1, 0xa2]);

    let refreshed = Record::access(&producer.bytes).expect("successful mutation stays valid");
    assert_eq!(
        (
            refreshed.sequence(),
            refreshed.header().version(),
            refreshed.header().active(),
            refreshed.samples().copy_into(),
            refreshed.token(),
        ),
        (9, 2, false, [19, 21, 23], &[0x10, 0x20, 0x30, 0x40])
    );
    println!(
        "record sequence={} samples={:?}",
        refreshed.sequence(),
        refreshed.samples().copy_into()
    );
}
