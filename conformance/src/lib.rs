#![deny(unsafe_op_in_unsafe_fn)]

mod ffi;
mod inventory;
mod report;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildCaseContract {
    pub(crate) case_id: u32,
    pub(crate) root_id: &'static str,
    pub(crate) layout_keys: &'static [u64],
    pub(crate) observation_keys: &'static [u64],
}

include!(concat!(env!("OUT_DIR"), "/case_contract.rs"));

pub use ffi::{
    cpp_inspect_fixture, cpp_inspect_fixture_into, cpp_layout_report, cpp_write_fixture,
    cpp_write_fixture_into,
};
pub use report::{FfiError, HarnessError, Report, Status};

#[doc(hidden)]
pub fn rust_fixture(case_id: u32) -> Result<Vec<u8>, HarnessError> {
    let case = inventory::CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .ok_or(HarnessError::InvalidData("unknown benchmark case"))?;
    (case.rust_bytes)()
}

#[doc(hidden)]
pub fn rust_observe(case_id: u32, bytes: &[u8]) -> Result<Vec<(u64, u64)>, HarnessError> {
    let case = inventory::CASES
        .iter()
        .find(|case| case.case_id == case_id)
        .ok_or(HarnessError::InvalidData("unknown benchmark case"))?;
    (case.rust_observe)(bytes)
}

#[cfg(all(test, not(miri)))]
mod tests;
