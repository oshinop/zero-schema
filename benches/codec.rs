use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use zero_schema::{AlignedBytes, ZeroSchema, ZeroSchemaType};

const STREAM_BYTES: usize = 64 * 1024 * 1024;
const WIRE_SIZE: usize = BenchmarkMessage::WIRE_SIZE;
const ACTIVE_BYTES: usize = active_bytes(PlainMessage::Payload(0));
type BenchmarkStorage =
    AlignedBytes<<BenchmarkMessage as ZeroSchemaType>::Wire, { BenchmarkMessage::WIRE_SIZE }>;
const RING_STEP: usize = 1_048_573;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ZeroSchema)]
struct BenchmarkPayload {
    value: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ZeroSchema)]
#[repr(u8)]
enum BenchmarkTag {
    Unit = 1,
    Payload = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ZeroSchema)]
#[zero(tag = BenchmarkTag, tail = "zero")]
enum BenchmarkMessage {
    #[zero(tag = BenchmarkTag::Unit)]
    Unit,
    #[zero(tag = BenchmarkTag::Payload)]
    Payload(BenchmarkPayload),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlainMessage {
    Unit,
    Payload(u32),
}

const fn payload_active_bytes(_: u32) -> usize {
    core::mem::size_of::<u32>()
}

const fn active_bytes(value: PlainMessage) -> usize {
    match value {
        PlainMessage::Unit => 1,
        PlainMessage::Payload(payload) => 1 + payload_active_bytes(payload),
    }
}

fn plain_encode(value: PlainMessage, destination: &mut [u8]) {
    destination.fill(0);
    match value {
        PlainMessage::Unit => destination[0] = 1,
        PlainMessage::Payload(value) => {
            destination[0] = 2;
            destination[4..8].copy_from_slice(&value.to_ne_bytes());
        }
    }
}

fn plain_decode(source: &[u8]) -> PlainMessage {
    match source[0] {
        1 if source[4..8].iter().all(|byte| *byte == 0) => PlainMessage::Unit,
        1 => panic!("nonzero inactive handwritten payload"),
        2 => PlainMessage::Payload(u32::from_ne_bytes(source[4..8].try_into().unwrap())),
        tag => panic!("invalid handwritten tag {tag}"),
    }
}

fn generated_value() -> BenchmarkMessage {
    BenchmarkMessage::Payload(BenchmarkPayload { value: 0x1122_3344 })
}

fn allocate_ring(slots: usize) -> Vec<BenchmarkStorage> {
    core::iter::repeat_with(|| zero_schema::make_buffer_for!(BenchmarkMessage))
        .take(slots)
        .collect()
}

fn validate_equivalence() {
    let generated = generated_value();
    let plain = PlainMessage::Payload(0x1122_3344);
    let mut generated_buffer = zero_schema::make_buffer_for!(BenchmarkMessage);
    let mut plain_buffer = zero_schema::make_buffer_for!(BenchmarkMessage);
    generated
        .encode_into(generated_buffer.as_bytes_mut())
        .unwrap();
    plain_encode(plain, plain_buffer.as_bytes_mut());
    assert_eq!(generated_buffer.as_bytes(), plain_buffer.as_bytes());
    assert_eq!(
        BenchmarkMessage::parse(plain_buffer.as_bytes()).unwrap(),
        generated
    );
    assert_eq!(plain_decode(generated_buffer.as_bytes()), plain);
    assert_eq!(WIRE_SIZE, 8);
    assert_eq!(STREAM_BYTES % WIRE_SIZE, 0);
    assert_eq!(RING_STEP % 2, 1); // slot count is a power of two, so the step is coprime.
}

fn codec(c: &mut Criterion) {
    validate_equivalence();
    let generated = generated_value();
    let plain = PlainMessage::Payload(0x1122_3344);

    let mut warm = c.benchmark_group("warm_one_slot");
    warm.throughput(Throughput::Bytes(ACTIVE_BYTES as u64));
    let mut generated_slot = zero_schema::make_buffer_for!(BenchmarkMessage);
    generated
        .encode_into(generated_slot.as_bytes_mut())
        .unwrap();
    warm.bench_function(
        BenchmarkId::new(
            "generated_encode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES}"),
        ),
        |b| {
            b.iter(|| {
                black_box(&generated)
                    .encode_into(black_box(generated_slot.as_bytes_mut()))
                    .unwrap()
            })
        },
    );
    warm.bench_function(
        BenchmarkId::new(
            "generated_decode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES}"),
        ),
        |b| {
            b.iter(|| {
                black_box(BenchmarkMessage::parse(black_box(generated_slot.as_bytes())).unwrap())
            })
        },
    );
    let mut plain_slot = zero_schema::make_buffer_for!(BenchmarkMessage);
    plain_encode(plain, plain_slot.as_bytes_mut());
    warm.bench_function(
        BenchmarkId::new(
            "handwritten_encode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES}"),
        ),
        |b| b.iter(|| plain_encode(black_box(plain), black_box(plain_slot.as_bytes_mut()))),
    );
    warm.bench_function(
        BenchmarkId::new(
            "handwritten_decode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES}"),
        ),
        |b| b.iter(|| black_box(plain_decode(black_box(plain_slot.as_bytes())))),
    );
    warm.finish();

    let slots = STREAM_BYTES / WIRE_SIZE;
    let mut generated_ring = allocate_ring(slots);
    for slot in &mut generated_ring {
        generated.encode_into(slot.as_bytes_mut()).unwrap();
    }
    let mut plain_ring = allocate_ring(slots);
    for slot in &mut plain_ring {
        plain_encode(plain, slot.as_bytes_mut());
    }
    let mut streaming = c.benchmark_group("streaming_64MiB");
    streaming.throughput(Throughput::Bytes(ACTIVE_BYTES as u64));
    let mut index = 0usize;
    streaming.bench_function(
        BenchmarkId::new(
            "generated_encode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES},step={RING_STEP}"),
        ),
        |b| {
            b.iter(|| {
                index = (index + RING_STEP) & (slots - 1);
                black_box(&generated)
                    .encode_into(black_box(generated_ring[index].as_bytes_mut()))
                    .unwrap()
            })
        },
    );
    index = 0;
    streaming.bench_function(
        BenchmarkId::new(
            "generated_decode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES},step={RING_STEP}"),
        ),
        |b| {
            b.iter(|| {
                index = (index + RING_STEP) & (slots - 1);
                black_box(
                    BenchmarkMessage::parse(black_box(generated_ring[index].as_bytes())).unwrap(),
                )
            })
        },
    );
    index = 0;
    streaming.bench_function(
        BenchmarkId::new(
            "handwritten_encode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES},step={RING_STEP}"),
        ),
        |b| {
            b.iter(|| {
                index = (index + RING_STEP) & (slots - 1);
                plain_encode(
                    black_box(plain),
                    black_box(plain_ring[index].as_bytes_mut()),
                )
            })
        },
    );
    index = 0;
    streaming.bench_function(
        BenchmarkId::new(
            "handwritten_decode",
            format!("size={WIRE_SIZE},active={ACTIVE_BYTES},step={RING_STEP}"),
        ),
        |b| {
            b.iter(|| {
                index = (index + RING_STEP) & (slots - 1);
                black_box(plain_decode(black_box(plain_ring[index].as_bytes())))
            })
        },
    );
    streaming.finish();
}

criterion_group!(benches, codec);
criterion_main!(benches);
