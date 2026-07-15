use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use zero_schema_conformance::{
    cpp_inspect_fixture, cpp_inspect_fixture_into, cpp_write_fixture, cpp_write_fixture_into,
    rust_observe,
};
use zero_schema_schema_corpus::conformance::ConformanceExternalMessage;

const CASE_ID: u32 = 1010;
const BATCH: usize = 1024;
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
    let wire_size = ConformanceExternalMessage::SCHEMA_SIZE;
    let producer_bytes = cpp_write_fixture(CASE_ID, wire_size).expect("C++ producer");
    let expected_observations = rust_observe(CASE_ID, &producer_bytes).expect("Rust access");
    assert_eq!(
        cpp_inspect_fixture(CASE_ID, &producer_bytes)
            .expect("C++ observation")
            .pairs(),
        expected_observations.as_slice()
    );

    let mut produced = vec![0_u8; wire_size];
    let mut slots = vec![0_u64; 3 + 2 * expected_observations.len()];
    assert_eq!(
        cpp_write_fixture_into(CASE_ID, &mut produced).expect("warm C++ producer"),
        wire_size
    );
    assert_eq!(
        cpp_inspect_fixture_into(CASE_ID, &producer_bytes, &mut slots).expect("warm C++ observer"),
        slots.len()
    );
    assert_eq!(
        rust_observe(CASE_ID, &producer_bytes).expect("warm Rust access"),
        expected_observations
    );

    let parameter = format!("case-{CASE_ID}/wire-{wire_size}/active-{ACTIVE_BYTES}");
    let mut group = c.benchmark_group("cpp_conformance");
    group.throughput(Throughput::Bytes(ACTIVE_BYTES as u64));

    group.bench_with_input(BenchmarkId::new("producer", &parameter), &(), |b, &()| {
        b.iter_custom(|iterations| {
            let start = Instant::now();
            for _ in 0..iterations {
                for _ in 0..BATCH {
                    black_box(
                        cpp_write_fixture_into(CASE_ID, &mut produced).expect("C++ producer"),
                    );
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

    group.bench_with_input(
        BenchmarkId::new("cpp-observe", &parameter),
        &(),
        |b, &()| {
            b.iter_custom(|iterations| {
                let start = Instant::now();
                for _ in 0..iterations {
                    for _ in 0..BATCH {
                        black_box(
                            cpp_inspect_fixture_into(CASE_ID, &producer_bytes, &mut slots)
                                .expect("C++ observer"),
                        );
                    }
                }
                let batch = start.elapsed();
                let empty_start = Instant::now();
                for _ in 0..iterations {
                    empty_batch();
                }
                subtract_baseline(batch, empty_start.elapsed())
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("rust-access", &parameter),
        &(),
        |b, &()| {
            b.iter_custom(|iterations| {
                let start = Instant::now();
                for _ in 0..iterations {
                    for _ in 0..BATCH {
                        black_box(rust_observe(CASE_ID, &producer_bytes).expect("Rust access"));
                    }
                }
                let batch = start.elapsed();
                let empty_start = Instant::now();
                for _ in 0..iterations {
                    empty_batch();
                }
                subtract_baseline(batch, empty_start.elapsed())
            });
        },
    );

    group.finish();
}

criterion_group!(benches, cpp_codec);
criterion_main!(benches);
