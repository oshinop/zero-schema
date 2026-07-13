#[cfg(test)]
use core::ptr;

use crate::BUILD_CASES;
use crate::report::{FfiError, HarnessError, Report, Status};

unsafe extern "C" {
    fn zs_layout_report(case_id: u32, output: *mut u64, capacity: usize, written: *mut usize)
    -> u8;
    fn zs_write_fixture(case_id: u32, output: *mut u8, capacity: usize, written: *mut usize) -> u8;
    fn zs_inspect_fixture(
        case_id: u32,
        input: *const u8,
        input_len: usize,
        output: *mut u64,
        capacity: usize,
        written: *mut usize,
    ) -> u8;
}

fn status(raw: u8) -> Result<(), FfiError> {
    match Status::try_from(raw)? {
        Status::Ok => Ok(()),
        other => Err(FfiError::Status(other)),
    }
}

fn contract(case_id: u32) -> Result<&'static crate::BuildCaseContract, FfiError> {
    BUILD_CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .ok_or(FfiError::Status(Status::UnknownId))
}

pub fn cpp_layout_report(case_id: u32) -> Result<Report, HarnessError> {
    let expected_keys = contract(case_id)?.layout_keys;
    let capacity = expected_keys
        .len()
        .checked_mul(2)
        .and_then(|pairs| pairs.checked_add(3))
        .ok_or(FfiError::Status(Status::CountOverflowOrInternal))?;
    let mut slots = vec![0_u64; capacity];
    let mut written = 0;
    // SAFETY: `slots` is initialized, u64-aligned, and has exactly `capacity`
    // writable elements; `written` is a valid, aligned `usize` output.
    let raw = unsafe { zs_layout_report(case_id, slots.as_mut_ptr(), capacity, &mut written) };
    status(raw)?;
    Report::parse(case_id, expected_keys, &slots, written)
}

pub fn cpp_write_fixture_into(case_id: u32, output: &mut [u8]) -> Result<usize, FfiError> {
    let mut written = 0;
    // SAFETY: the slice pointer denotes `output.len()` writable initialized bytes,
    // and `written` is a valid, aligned `usize` output.
    let raw = unsafe { zs_write_fixture(case_id, output.as_mut_ptr(), output.len(), &mut written) };
    status(raw)?;
    if written > output.len() {
        return Err(FfiError::Status(Status::CountOverflowOrInternal));
    }
    Ok(written)
}

pub fn cpp_inspect_fixture_into(
    case_id: u32,
    input: &[u8],
    output: &mut [u64],
) -> Result<usize, FfiError> {
    let expected_keys = contract(case_id)?.observation_keys;
    let required = expected_keys
        .len()
        .checked_mul(2)
        .and_then(|pairs| pairs.checked_add(3))
        .ok_or(FfiError::Status(Status::CountOverflowOrInternal))?;
    if output.len() < required {
        return Err(FfiError::Status(Status::InsufficientCapacity));
    }
    let mut written = 0;
    // SAFETY: `input` denotes `input.len()` readable bytes; `output` is initialized,
    // u64-aligned, and has `output.len()` writable elements; `written` is valid and
    // aligned. The C++ function receives the exact capacities of both slices.
    let raw = unsafe {
        zs_inspect_fixture(
            case_id,
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut written,
        )
    };
    status(raw)?;
    if written != required || written > output.len() {
        return Err(FfiError::Status(Status::CountOverflowOrInternal));
    }
    Ok(written)
}

pub fn cpp_write_fixture(case_id: u32, expected_len: usize) -> Result<Vec<u8>, FfiError> {
    let mut output = vec![0_u8; expected_len];
    let written = cpp_write_fixture_into(case_id, &mut output)?;
    if written != expected_len {
        return Err(FfiError::Status(Status::CountOverflowOrInternal));
    }
    Ok(output)
}

pub fn cpp_inspect_fixture(case_id: u32, input: &[u8]) -> Result<Report, HarnessError> {
    let expected_keys = contract(case_id)?.observation_keys;
    let capacity = expected_keys
        .len()
        .checked_mul(2)
        .and_then(|pairs| pairs.checked_add(3))
        .ok_or(FfiError::Status(Status::CountOverflowOrInternal))?;
    let mut slots = vec![0_u64; capacity];
    let mut written = 0;
    // SAFETY: `input` denotes `input.len()` readable bytes; `slots` is initialized,
    // u64-aligned, and has `capacity` writable elements; `written` is valid/aligned.
    let raw = unsafe {
        zs_inspect_fixture(
            case_id,
            input.as_ptr(),
            input.len(),
            slots.as_mut_ptr(),
            capacity,
            &mut written,
        )
    };
    status(raw)?;
    Report::parse(case_id, expected_keys, &slots, written)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProbePointer {
    Valid,
    Null,
    Misaligned,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ProbeResult<T> {
    pub raw_status: u8,
    pub written: Option<usize>,
    pub storage: Vec<T>,
}

#[cfg(test)]
const SENTINEL_U64: u64 = 0xd3d3_d3d3_d3d3_d3d3;
#[cfg(test)]
const SENTINEL_U8: u8 = 0xd3;

#[cfg(test)]
#[repr(C)]
struct WrittenStorage {
    value: usize,
    extra: u8,
}

#[cfg(test)]
fn written_pointer(kind: ProbePointer, storage: &mut WrittenStorage) -> *mut usize {
    match kind {
        ProbePointer::Valid => &mut storage.value,
        ProbePointer::Null => ptr::null_mut(),
        // `storage` supplies excess bytes; C++ checks alignment before dereferencing.
        ProbePointer::Misaligned => unsafe {
            (&mut storage.value as *mut usize)
                .cast::<u8>()
                .add(1)
                .cast()
        },
    }
}

#[cfg(test)]
fn written_value(kind: ProbePointer, storage: &WrittenStorage) -> Option<usize> {
    match kind {
        ProbePointer::Valid => Some(storage.value),
        ProbePointer::Null | ProbePointer::Misaligned => None,
    }
}

#[cfg(test)]
fn report_output_pointer(storage: &mut [u64], kind: ProbePointer) -> *mut u64 {
    match kind {
        ProbePointer::Valid => unsafe { storage.as_mut_ptr().add(1) },
        ProbePointer::Null => ptr::null_mut(),
        // The allocation has guard space; C++ checks alignment before writing.
        ProbePointer::Misaligned => unsafe { storage.as_mut_ptr().cast::<u8>().add(1).cast() },
    }
}

#[cfg(test)]
pub(crate) fn probe_layout(
    id: u32,
    capacity: usize,
    written: ProbePointer,
    output: ProbePointer,
) -> ProbeResult<u64> {
    let storage_len = if matches!(output, ProbePointer::Valid) {
        capacity.checked_add(2).expect("capacity overflow")
    } else {
        2
    };
    let mut storage = vec![SENTINEL_U64; storage_len];
    let mut written_storage = WrittenStorage {
        value: usize::MAX,
        extra: SENTINEL_U8,
    };
    let written_ptr = written_pointer(written, &mut written_storage);
    let output_ptr = report_output_pointer(&mut storage, output);
    // SAFETY: valid probes point into sufficiently large initialized allocations;
    // null and deliberately misaligned probes are defined fault inputs that the ABI
    // checks before dereference. `capacity` never exceeds the output allocation.
    let raw_status = unsafe { zs_layout_report(id, output_ptr, capacity, written_ptr) };
    ProbeResult {
        raw_status,
        written: written_value(written, &written_storage),
        storage,
    }
}

#[cfg(test)]
pub(crate) fn probe_write(
    id: u32,
    capacity: usize,
    written: ProbePointer,
    output: ProbePointer,
) -> ProbeResult<u8> {
    let storage_len = if matches!(output, ProbePointer::Valid | ProbePointer::Misaligned) {
        capacity.checked_add(2).expect("capacity overflow")
    } else {
        2
    };
    let mut storage = vec![SENTINEL_U8; storage_len];
    let mut written_storage = WrittenStorage {
        value: usize::MAX,
        extra: SENTINEL_U8,
    };
    let written_ptr = written_pointer(written, &mut written_storage);
    let output_ptr = match output {
        ProbePointer::Valid => unsafe { storage.as_mut_ptr().add(1) },
        ProbePointer::Null => ptr::null_mut(),
        // A byte pointer has alignment one, so an offset pointer is still valid.
        ProbePointer::Misaligned => unsafe { storage.as_mut_ptr().add(1) },
    };
    // SAFETY: valid byte probes have `capacity` writable bytes after the leading
    // guard; null is a defined fault input; `written_ptr` follows the probe contract.
    let raw_status = unsafe { zs_write_fixture(id, output_ptr, capacity, written_ptr) };
    ProbeResult {
        raw_status,
        written: written_value(written, &written_storage),
        storage,
    }
}

#[cfg(test)]
pub(crate) fn probe_inspect(
    id: u32,
    input: &[u8],
    advertised_len: usize,
    null_input: bool,
    capacity: usize,
    written: ProbePointer,
    output: ProbePointer,
) -> ProbeResult<u64> {
    let storage_len = if matches!(output, ProbePointer::Valid) {
        capacity.checked_add(2).expect("capacity overflow")
    } else {
        2
    };
    let mut storage = vec![SENTINEL_U64; storage_len];
    let mut written_storage = WrittenStorage {
        value: usize::MAX,
        extra: SENTINEL_U8,
    };
    let written_ptr = written_pointer(written, &mut written_storage);
    let output_ptr = report_output_pointer(&mut storage, output);
    let input_ptr = if null_input {
        ptr::null()
    } else {
        input.as_ptr()
    };
    // SAFETY: nonnull input points to `input`; tests advertise only lengths intended
    // for ABI precedence checks (C++ validates the exact length before reading).
    // Output and written probes obey the same allocation/null/alignment proof above.
    let raw_status = unsafe {
        zs_inspect_fixture(
            id,
            input_ptr,
            advertised_len,
            output_ptr,
            capacity,
            written_ptr,
        )
    };
    ProbeResult {
        raw_status,
        written: written_value(written, &written_storage),
        storage,
    }
}
