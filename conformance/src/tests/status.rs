use crate::ffi::{ProbePointer, cpp_write_fixture, probe_inspect, probe_layout, probe_write};
use crate::inventory::CASES;
use crate::report::{FfiError, Status};

const ID: u32 = 1001;

fn producer_input() -> Vec<u8> {
    let case = CASES
        .iter()
        .find(|case| case.case_id == ID)
        .expect("case 1001");
    cpp_write_fixture(ID, (case.schema_size)()).expect("C++ producer")
}

fn assert_failed<T: Eq + core::fmt::Debug>(
    raw: u8,
    expected: Status,
    written: Option<usize>,
    storage: &[T],
) {
    assert_eq!(raw, expected as u8);
    if written.is_some() {
        assert_eq!(written, Some(0));
    }
    assert!(
        storage.windows(2).all(|window| window[0] == window[1]),
        "failure modified output sentinel"
    );
}

#[test]
fn every_status_and_unknown_mapping_is_stable() {
    let statuses = [
        Status::Ok,
        Status::NullWritten,
        Status::UnknownId,
        Status::InvalidInputLength,
        Status::NullInput,
        Status::NullOutput,
        Status::InsufficientCapacity,
        Status::CountOverflowOrInternal,
    ];
    for (raw, status) in statuses.into_iter().enumerate() {
        assert_eq!(Status::try_from(raw as u8), Ok(status));
    }
    assert_eq!(Status::try_from(8), Err(FfiError::UnknownStatus(8)));
    assert_eq!(Status::try_from(255), Err(FfiError::UnknownStatus(255)));
}

#[test]
fn written_pointer_faults_have_first_precedence() {
    for written in [ProbePointer::Null, ProbePointer::Misaligned] {
        let result = probe_layout(u32::MAX, 0, written, ProbePointer::Null);
        assert_eq!(result.raw_status, Status::NullWritten as u8);
        assert_eq!(result.written, None);
    }
}

#[test]
fn layout_precedence_capacity_null_and_alignment() {
    let unknown = probe_layout(u32::MAX, 0, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        unknown.raw_status,
        Status::UnknownId,
        unknown.written,
        &unknown.storage,
    );
    let short = probe_layout(ID, 0, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        short.raw_status,
        Status::InsufficientCapacity,
        short.written,
        &short.storage,
    );
    let null = probe_layout(ID, usize::MAX, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        null.raw_status,
        Status::NullOutput,
        null.written,
        &null.storage,
    );
    let misaligned = probe_layout(
        ID,
        usize::MAX,
        ProbePointer::Valid,
        ProbePointer::Misaligned,
    );
    assert_failed(
        misaligned.raw_status,
        Status::CountOverflowOrInternal,
        misaligned.written,
        &misaligned.storage,
    );
}

#[test]
fn write_capacity_boundaries_and_sentinels() {
    let required = (CASES[0].schema_size)();
    let short = probe_write(ID, required - 1, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        short.raw_status,
        Status::InsufficientCapacity,
        short.written,
        &short.storage,
    );
    let exact = probe_write(ID, required, ProbePointer::Valid, ProbePointer::Valid);
    assert_eq!(exact.raw_status, Status::Ok as u8);
    assert_eq!(exact.written, Some(required));
    let excess = probe_write(ID, required + 7, ProbePointer::Valid, ProbePointer::Valid);
    assert_eq!(excess.raw_status, Status::Ok as u8);
    assert_eq!(excess.written, Some(required));
    assert_eq!(
        &exact.storage[1..required + 1],
        &excess.storage[1..required + 1]
    );
    assert!(
        excess.storage[required + 1..]
            .windows(2)
            .all(|w| w[0] == w[1])
    );
}

#[test]
fn inspect_simultaneous_fault_precedence_and_immutability() {
    let input = producer_input();
    let original = input.clone();
    let unknown = probe_inspect(
        u32::MAX,
        &input,
        0,
        true,
        0,
        ProbePointer::Valid,
        ProbePointer::Null,
    );
    assert_failed(
        unknown.raw_status,
        Status::UnknownId,
        unknown.written,
        &unknown.storage,
    );
    let wrong_len = probe_inspect(
        ID,
        &input,
        input.len() - 1,
        true,
        0,
        ProbePointer::Valid,
        ProbePointer::Null,
    );
    assert_failed(
        wrong_len.raw_status,
        Status::InvalidInputLength,
        wrong_len.written,
        &wrong_len.storage,
    );
    let null_input = probe_inspect(
        ID,
        &input,
        input.len(),
        true,
        0,
        ProbePointer::Valid,
        ProbePointer::Null,
    );
    assert_failed(
        null_input.raw_status,
        Status::NullInput,
        null_input.written,
        &null_input.storage,
    );
    let short = probe_inspect(
        ID,
        &input,
        input.len(),
        false,
        0,
        ProbePointer::Valid,
        ProbePointer::Null,
    );
    assert_failed(
        short.raw_status,
        Status::InsufficientCapacity,
        short.written,
        &short.storage,
    );
    let null_output = probe_inspect(
        ID,
        &input,
        input.len(),
        false,
        usize::MAX,
        ProbePointer::Valid,
        ProbePointer::Null,
    );
    assert_failed(
        null_output.raw_status,
        Status::NullOutput,
        null_output.written,
        &null_output.storage,
    );
    let misaligned = probe_inspect(
        ID,
        &input,
        input.len(),
        false,
        usize::MAX,
        ProbePointer::Valid,
        ProbePointer::Misaligned,
    );
    assert_failed(
        misaligned.raw_status,
        Status::CountOverflowOrInternal,
        misaligned.written,
        &misaligned.storage,
    );
    assert_eq!(input, original, "inspect modified input");
}

#[test]
fn inspect_exact_and_excess_capacity_write_only_required_slots() {
    let input = producer_input();
    let pairs = crate::BUILD_CASES[0].observation_keys.len();
    let required = 3 + 2 * pairs;
    for capacity in [required, required + 5] {
        let result = probe_inspect(
            ID,
            &input,
            input.len(),
            false,
            capacity,
            ProbePointer::Valid,
            ProbePointer::Valid,
        );
        assert_eq!(result.raw_status, Status::Ok as u8);
        assert_eq!(result.written, Some(required));
        assert_eq!(&result.storage[1..4], &[1, ID as u64, pairs as u64]);
        assert!(
            result.storage[required + 1..]
                .windows(2)
                .all(|w| w[0] == w[1])
        );
    }
}

#[test]
fn write_fault_precedence_and_written_reset_matrix() {
    let required = (CASES[0].schema_size)();
    for written in [ProbePointer::Null, ProbePointer::Misaligned] {
        let result = probe_write(u32::MAX, 0, written, ProbePointer::Null);
        assert_eq!(result.raw_status, Status::NullWritten as u8);
        assert_eq!(result.written, None);
    }
    let unknown = probe_write(u32::MAX, 0, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        unknown.raw_status,
        Status::UnknownId,
        unknown.written,
        &unknown.storage,
    );
    let short_null = probe_write(ID, required - 1, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        short_null.raw_status,
        Status::InsufficientCapacity,
        short_null.written,
        &short_null.storage,
    );
    let null = probe_write(ID, required, ProbePointer::Valid, ProbePointer::Null);
    assert_failed(
        null.raw_status,
        Status::NullOutput,
        null.written,
        &null.storage,
    );
}
