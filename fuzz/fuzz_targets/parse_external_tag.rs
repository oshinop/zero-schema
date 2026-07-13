#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| zero_schema_fuzz::parse_external_tag(data));
