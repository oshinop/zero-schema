use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use zero_schema_conformance::{
    cpp_inspect_fixture, cpp_inspect_fixture_into, cpp_write_fixture_into, rust_fixture,
    rust_observe,
};
use zero_schema_schema_corpus::conformance::ConformanceExternalMessage;

const CASE_ID: u32 = 1010;
const BATCH: usize = 1024;
// prefix u8 + external u8 tag + selected ConformanceData u32 + suffix u16.
const ACTIVE_BYTES: usize = 1 + 1 + 4 + 2;

fn empty_batch() {
    for index in 0..BATCH {
        black_box(index);
    }
}

fn subtract_baseline(batch: Duration, empty: Duration) -> Duration {
    batch.saturating_sub(empty) / u32::try_from(BATCH).expect("batch fits u32")
}

fn cpp_codec(c: &mut Criterion) {
    let wire_size = ConformanceExternalMessage::WIRE_SIZE;
    let rust_bytes = rust_fixture(CASE_ID).expect("Rust fixture must encode");
    assert_eq!(rust_bytes.len(), wire_size);

    let mut encoded = vec![0_u8; wire_size];
    let written = cpp_write_fixture_into(CASE_ID, &mut encoded).expect("C++ fixture must encode");
    assert_eq!(written, wire_size);
    assert_eq!(encoded, rust_bytes, "C++ bytes must match Rust bytes");

    let expected_observations =
        rust_observe(CASE_ID, &rust_bytes).expect("Rust fixture must decode");
    let report = cpp_inspect_fixture(CASE_ID, &rust_bytes).expect("C++ fixture must decode");
    assert_eq!(report.pairs(), expected_observations.as_slice());
    let report_slots = 3 + 2 * report.pairs().len();
    let mut slots = vec![0_u64; report_slots];

    // Warm both C++ paths once before Criterion starts measuring.
    assert_eq!(
        cpp_write_fixture_into(CASE_ID, black_box(&mut encoded)).expect("warm encode"),
        wire_size
    );
    assert_eq!(
        cpp_inspect_fixture_into(CASE_ID, black_box(&encoded), black_box(&mut slots))
            .expect("warm inspect"),
        report_slots
    );

    let parameter = format!("case-{CASE_ID}/wire-{wire_size}/active-{ACTIVE_BYTES}");
    let mut group = c.benchmark_group("cpp_codec");
    group.throughput(Throughput::Bytes(ACTIVE_BYTES as u64));

    group.bench_with_input(BenchmarkId::new("write", &parameter), &(), |b, &()| {
        b.iter_custom(|iterations| {
            let start = Instant::now();
            for _ in 0..iterations {
                for _ in 0..BATCH {
                    let count = cpp_write_fixture_into(CASE_ID, black_box(&mut encoded))
                        .expect("C++ encode failed");
                    black_box(count);
                }
            }
            let batch = start.elapsed();
            let empty_start = Instant::now();
            for _ in 0..iterations {
                empty_batch();
            }
            subtract_baseline(batch, empty_start.elapsed())
        });
    });

    group.bench_with_input(BenchmarkId::new("inspect", &parameter), &(), |b, &()| {
        b.iter_custom(|iterations| {
            let start = Instant::now();
            for _ in 0..iterations {
                for _ in 0..BATCH {
                    let count = cpp_inspect_fixture_into(
                        CASE_ID,
                        black_box(&encoded),
                        black_box(&mut slots),
                    )
                    .expect("C++ inspect failed");
                    black_box(count);
                }
            }
            let batch = start.elapsed();
            let empty_start = Instant::now();
            for _ in 0..iterations {
                empty_batch();
            }
            subtract_baseline(batch, empty_start.elapsed())
        });
    });

    group.finish();

    let mut baseline_group = c.benchmark_group("cpp_codec_empty_baseline");
    baseline_group.bench_with_input(BenchmarkId::new("empty", &parameter), &(), |b, &()| {
        b.iter(empty_batch);
    });
    baseline_group.finish();
}

criterion_group!(benches, cpp_codec);
criterion_main!(benches);
