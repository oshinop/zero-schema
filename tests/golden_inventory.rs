use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use zero_schema_schema_corpus::{FUZZ_TARGETS, ROOT_IDS};

const HEADER: &str = "root_id,type_key,fuzz_target,selector,golden_path,golden_len,golden_sha256,valid_seed_path,valid_seed_sha256,invalid_seed_path,invalid_seed_sha256";

const ROOT_TYPES: &[(&str, &str)] = &[
    ("1", "CorpusCode16Be"),
    ("2", "ExternalCorpusMessage"),
    ("3", "CorpusCode8"),
    ("4", "ExternalCorpusMessage"),
    ("5", "FuzzAllStrings"),
    ("6", "AllFeatures"),
];

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn inventory_is_exact_complete_and_hashed() {
    let manifest = include_bytes!("../test-fixtures/schema-corpus/inventory.csv");
    assert!(!manifest.contains(&b'\r'), "inventory must be LF-only");
    assert_eq!(manifest.last(), Some(&b'\n'));
    let text = core::str::from_utf8(manifest).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(HEADER));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut ids = BTreeSet::new();
    let mut registrations = BTreeSet::new();
    let mut type_keys = BTreeMap::new();
    let mut selectors: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for line in lines {
        assert!(!line.is_empty() && !line.starts_with('#'));
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(columns.len(), 11, "malformed row: {line}");
        assert!(columns.iter().all(|column| !column.is_empty()));
        assert!(ids.insert(columns[0]));
        assert!(type_keys.insert(columns[0], columns[1]).is_none());
        assert!(
            columns[1]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        );
        let selector: u8 = columns[3].parse().unwrap();
        assert!((1..=256).contains(&usize::from(selector)));
        registrations.insert((columns[0], columns[2], selector));
        selectors.entry(columns[2]).or_default().push(selector);
        let length: usize = columns[5].parse().unwrap();
        for path_column in [4, 7, 9] {
            let path = columns[path_column];
            assert!(!path.starts_with('/') && !path.split('/').any(|part| part == ".."));
        }
        for digest_column in [6, 8, 10] {
            let digest = columns[digest_column];
            assert_eq!(digest.len(), 64);
            assert!(
                digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }
        let golden = fs::read(root.join(columns[4])).unwrap();
        assert_eq!(golden.len(), length);
        assert_eq!(hex(Sha256::digest(&golden)), columns[6]);
        let valid_seed = fs::read(root.join(columns[7])).unwrap();
        assert_eq!(hex(Sha256::digest(&valid_seed)), columns[8]);
        if columns[2] == "parse_message" {
            assert_eq!(valid_seed.first(), Some(&(selector - 1)));
            assert_eq!(&valid_seed[1..], golden.as_slice());
        } else {
            assert_eq!(valid_seed, golden);
        }
        let invalid_seed = fs::read(root.join(columns[9])).unwrap();
        assert_eq!(hex(Sha256::digest(&invalid_seed)), columns[10]);
    }
    for values in selectors.values_mut() {
        values.sort_unstable();
        assert_eq!(*values, (1..=values.len() as u8).collect::<Vec<_>>());
    }
    assert_eq!(ids, ROOT_IDS.iter().copied().collect());
    assert_eq!(type_keys, ROOT_TYPES.iter().copied().collect());
    assert_eq!(registrations, FUZZ_TARGETS.iter().copied().collect());
    assert!(selectors.contains_key("parse_external_tag"));
}
