use zero_schema_schema_corpus::{
    CorpusCode8, CorpusCode16Be, CorpusMessage, ExternalCorpusMessage, FuzzAllStrings,
};

pub const TARGET_COUNTS: &[(&str, usize)] = &[
    ("parse_message", 3),
    ("parse_external_tag", 1),
    ("parse_all_strings", 1),
    ("roundtrip_message", 1),
];

fn copy_input(destination: &mut [u8], payload: &[u8]) {
    destination.fill(0);
    let count = destination.len().min(payload.len());
    destination[..count].copy_from_slice(&payload[..count]);
}

fn stable<T: PartialEq + core::fmt::Debug>(
    left: Result<T, impl core::fmt::Debug>,
    right: Result<T, impl core::fmt::Debug>,
) -> bool {
    match (left, right) {
        (Ok(left), Ok(right)) => {
            assert_eq!(left, right);
            true
        }
        (Err(left), Err(right)) => {
            assert_eq!(format!("{left:?}"), format!("{right:?}"));
            false
        }
        _ => panic!("nondeterministic parse result"),
    }
}

fn parse_code8(payload: &[u8]) -> bool {
    let mut buffer = zero_schema::make_buffer_for!(CorpusCode8);
    copy_input(buffer.as_bytes_mut(), payload);
    stable(
        CorpusCode8::parse(buffer.as_bytes()),
        CorpusCode8::parse(buffer.as_bytes()),
    )
}

fn parse_code16(payload: &[u8]) -> bool {
    let mut buffer = zero_schema::make_buffer_for!(CorpusCode16Be);
    copy_input(buffer.as_bytes_mut(), payload);
    stable(
        CorpusCode16Be::parse(buffer.as_bytes()),
        CorpusCode16Be::parse(buffer.as_bytes()),
    )
}

fn parse_message_value(payload: &[u8], roundtrip: bool) -> bool {
    let mut buffer = zero_schema::make_buffer_for!(CorpusMessage);
    copy_input(buffer.as_bytes_mut(), payload);
    let first = CorpusMessage::parse(buffer.as_bytes());
    let second = CorpusMessage::parse(buffer.as_bytes());
    match (first, second) {
        (Ok(value), Ok(observed)) => {
            assert_eq!(value, observed);
            if roundtrip {
                let encoded = value.encode().unwrap();
                assert_eq!(value, CorpusMessage::parse(encoded.as_bytes()).unwrap());
            }
            true
        }
        (Err(left), Err(right)) => {
            assert_eq!(format!("{left:?}"), format!("{right:?}"));
            false
        }
        _ => panic!("nondeterministic parse result"),
    }
}

fn parse_external(payload: &[u8]) -> bool {
    let mut buffer = zero_schema::make_buffer_for!(ExternalCorpusMessage);
    copy_input(buffer.as_bytes_mut(), payload);
    stable(
        ExternalCorpusMessage::parse(buffer.as_bytes()),
        ExternalCorpusMessage::parse(buffer.as_bytes()),
    )
}

fn parse_strings(payload: &[u8]) -> bool {
    let mut buffer = zero_schema::make_buffer_for!(FuzzAllStrings);
    copy_input(buffer.as_bytes_mut(), payload);
    stable(
        FuzzAllStrings::parse(buffer.as_bytes()),
        FuzzAllStrings::parse(buffer.as_bytes()),
    )
}

fn split(input: &[u8], count: usize) -> (usize, &[u8]) {
    (
        usize::from(input.first().copied().unwrap_or(0)) % count,
        input.get(1..).unwrap_or(&[]),
    )
}

fn dispatch_accepts(target: &str, input: &[u8]) -> bool {
    match target {
        "parse_message" => {
            let (selector, payload) = split(input, 3);
            match selector {
                0 => parse_code8(payload),
                1 => parse_code16(payload),
                2 => parse_message_value(payload, false),
                _ => unreachable!(),
            }
        }
        "parse_external_tag" => parse_external(split(input, 1).1),
        "parse_all_strings" => parse_strings(split(input, 1).1),
        "roundtrip_message" => parse_message_value(split(input, 1).1, true),
        _ => panic!("unknown fuzz target"),
    }
}

pub fn parse_message(input: &[u8]) {
    let _ = dispatch_accepts("parse_message", input);
}
pub fn parse_external_tag(input: &[u8]) {
    let _ = dispatch_accepts("parse_external_tag", input);
}
pub fn parse_all_strings(input: &[u8]) {
    let _ = dispatch_accepts("parse_all_strings", input);
}
pub fn roundtrip_message(input: &[u8]) {
    let _ = dispatch_accepts("roundtrip_message", input);
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::{Path, PathBuf},
    };

    const HEADER: &str = "root_id,type_key,fuzz_target,selector,golden_path,golden_len,golden_sha256,valid_seed_path,valid_seed_sha256,invalid_seed_path,invalid_seed_sha256";

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned()
    }
    fn hash(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    fn checked_file(root: &Path, relative: &str, expected_hash: &str) -> Vec<u8> {
        assert!(
            !relative.is_empty()
                && !relative.starts_with('/')
                && !relative.split('/').any(|p| p == "..")
        );
        let bytes = fs::read(root.join(relative)).unwrap();
        assert_eq!(hash(&bytes), expected_hash);
        bytes
    }

    #[test]
    fn inventory_files_hashes_and_seed_semantics_are_strict() {
        let root = workspace();
        let text =
            fs::read_to_string(root.join("test-fixtures/schema-corpus/inventory.csv")).unwrap();
        assert!(!text.contains('\r') && text.ends_with('\n'));
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(HEADER));
        let mut selectors: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for line in lines {
            assert!(!line.is_empty() && !line.starts_with('#') && !line.contains('"'));
            let columns: Vec<_> = line.split(',').collect();
            assert_eq!(columns.len(), 11);
            let root_id: u32 = columns[0].parse().unwrap();
            assert!(roots.insert(root_id));
            let target = columns[2];
            let selector: usize = columns[3].parse().unwrap();
            assert!((1..=256).contains(&selector));
            selectors.entry(target).or_default().insert(selector);
            let golden = checked_file(&root, columns[4], columns[6]);
            assert_eq!(golden.len(), columns[5].parse::<usize>().unwrap());
            assert_eq!(
                columns[4],
                format!("test-fixtures/schema-corpus/golden/{root_id}.bin")
            );
            for digest in [columns[6], columns[8], columns[10]] {
                assert_eq!(digest.len(), 64);
                assert!(
                    digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                );
            }
            assert_eq!(columns[7], format!("fuzz/corpus/{target}/{selector}-valid"));
            assert_eq!(
                columns[9],
                format!("fuzz/corpus/{target}/{selector}-invalid")
            );
            let valid = checked_file(&root, columns[7], columns[8]);
            let invalid = checked_file(&root, columns[9], columns[10]);
            assert!(
                dispatch_accepts(target, &valid),
                "valid seed rejected: {target}/{selector}"
            );
            assert!(
                !dispatch_accepts(target, &invalid),
                "invalid seed accepted: {target}/{selector}"
            );
        }
        for &(target, count) in TARGET_COUNTS {
            assert_eq!(selectors.remove(target).unwrap(), (1..=count).collect());
        }
        assert!(selectors.is_empty());
        assert_eq!(roots.len(), zero_schema_schema_corpus::ROOT_IDS.len());
    }

    #[test]
    fn external_and_all_string_dispatch_reject_semantic_corruption() {
        assert!(!dispatch_accepts("parse_external_tag", &[1, 0xff]));
        let mut wide_length = vec![0; 1 + FuzzAllStrings::WIRE_SIZE];
        wide_length[0] = 1;
        wide_length[1 + 8] = 0xff;
        assert!(!dispatch_accepts("parse_all_strings", &wide_length));
        let mut wide_c_missing_nul = vec![0; 1 + FuzzAllStrings::WIRE_SIZE];
        wide_c_missing_nul[0] = 1;
        wide_c_missing_nul[1 + 16..].fill(1);
        assert!(!dispatch_accepts("parse_all_strings", &wide_c_missing_nul));
    }

    #[test]
    fn raw_input_boundaries_are_deterministic() {
        for dispatch in [
            parse_message as fn(&[u8]),
            parse_external_tag,
            parse_all_strings,
            roundtrip_message,
        ] {
            dispatch(&[]);
            dispatch(&[255]);
            dispatch(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
            dispatch(&[2, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        }
    }
}
