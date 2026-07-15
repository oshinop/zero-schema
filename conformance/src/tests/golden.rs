use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::BUILD_PROFILE;
use crate::ffi::cpp_write_fixture;
use crate::inventory::CASES;

const PROFILES: [&str; 5] = [
    "linux-x86_64-le",
    "linux-i686-le",
    "macos-aarch64-le",
    "windows-x86_64-msvc-le",
    "linux-powerpc64-be",
];
const CASE_IDS: [u32; 10] = [1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1010, 1011];
const HEADER: &str = "profile,case_id,path,length,sha256\n";

#[derive(Clone, Debug)]
struct Row {
    profile: String,
    case_id: u32,
    path: String,
    length: usize,
    hash: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn manifest_path() -> PathBuf {
    root().join("fixtures/golden/manifest.csv")
}
fn hash(bytes: &[u8]) -> String {
    use core::fmt::Write;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

fn parse_manifest(bytes: &[u8]) -> Vec<Row> {
    assert!(!bytes.contains(&b'\r'), "manifest must use LF only");
    let text = core::str::from_utf8(bytes).expect("manifest UTF-8");
    assert!(text.starts_with(HEADER), "exact manifest header");
    assert!(text.ends_with('\n'), "manifest must end in LF");
    let mut rows = Vec::new();
    for (line_no, line) in text.lines().skip(1).enumerate() {
        assert!(!line.is_empty(), "blank manifest row {}", line_no + 2);
        assert!(!line.starts_with('#'), "comments are forbidden");
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(columns.len(), 5, "five columns on row {}", line_no + 2);
        assert!(columns.iter().all(|value| !value.contains('"')));
        let case_id = columns[1].parse::<u32>().expect("decimal case ID");
        let length = columns[3].parse::<usize>().expect("decimal length");
        assert!(CASE_IDS.contains(&case_id));
        assert_ne!(length, 0);
        assert_eq!(columns[4].len(), 64);
        assert!(
            columns[4]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert!(!columns[2].split('/').any(|part| part == ".."));
        let expected_path = format!("conformance/fixtures/golden/{}/{case_id}.bin", columns[0]);
        assert_eq!(columns[2], expected_path);
        rows.push(Row {
            profile: columns[0].into(),
            case_id,
            path: columns[2].into(),
            length,
            hash: columns[4].into(),
        });
    }
    rows
}

fn file_path(row: &Row) -> PathBuf {
    root().parent().unwrap().join(&row.path)
}

#[test]
fn manifest_rows_files_and_hashes_are_exact() {
    let rows = parse_manifest(&fs::read(manifest_path()).expect("golden manifest"));
    assert_eq!(rows.len(), PROFILES.len() * CASE_IDS.len());
    let mut identities = BTreeSet::new();
    let mut declared_files = BTreeSet::new();
    for profile in PROFILES {
        let profile_rows: Vec<_> = rows.iter().filter(|row| row.profile == profile).collect();
        assert_eq!(profile_rows.len(), CASE_IDS.len());
        for (&case_id, row) in CASE_IDS.iter().zip(profile_rows) {
            assert_eq!(row.case_id, case_id);
            assert!(identities.insert((row.profile.clone(), row.case_id)));
            let path = file_path(row);
            let bytes = fs::read(&path).expect("declared golden file");
            assert_eq!(bytes.len(), row.length);
            assert_eq!(hash(&bytes), row.hash);
            declared_files.insert(path);
        }
    }
    let mut actual_files = BTreeSet::new();
    for profile in PROFILES {
        for entry in
            fs::read_dir(root().join("fixtures/golden").join(profile)).expect("profile directory")
        {
            let path = entry.expect("directory entry").path();
            assert!(path.is_file());
            actual_files.insert(path);
        }
    }
    assert_eq!(actual_files, declared_files);
}

#[test]
fn current_profile_is_reproduced_by_the_cxx_producer() {
    let rows = parse_manifest(&fs::read(manifest_path()).expect("golden manifest"));
    let current: BTreeMap<_, _> = rows
        .iter()
        .filter(|row| row.profile == BUILD_PROFILE)
        .map(|row| (row.case_id, row))
        .collect();
    assert_eq!(
        current.len(),
        CASE_IDS.len(),
        "current profile must be reviewed"
    );
    for case in CASES {
        let bytes = cpp_write_fixture(case.case_id, (case.schema_size)()).expect("C++ producer");
        assert_eq!(fs::read(file_path(current[&case.case_id])).unwrap(), bytes);
    }
}

#[test]
fn reviewed_profiles_have_the_frozen_case_identity_set() {
    let rows = parse_manifest(&fs::read(manifest_path()).expect("golden manifest"));
    for profile in PROFILES {
        let ids: Vec<_> = rows
            .iter()
            .filter(|row| row.profile == profile)
            .map(|row| row.case_id)
            .collect();
        assert_eq!(ids, CASE_IDS, "{profile} profile case IDs");
    }
}

#[test]
#[ignore = "explicit reviewed current-profile golden update"]
fn regenerate_current_profile() {
    assert_eq!(
        std::env::var("ZERO_SCHEMA_ACCEPT_GOLDENS").as_deref(),
        Ok(BUILD_PROFILE)
    );
    let path = manifest_path();
    let mut rows = parse_manifest(&fs::read(&path).expect("existing reviewed manifest"));
    for case in CASES {
        let bytes = cpp_write_fixture(case.case_id, (case.schema_size)()).expect("C++ producer");
        let row = rows
            .iter_mut()
            .find(|row| row.profile == BUILD_PROFILE && row.case_id == case.case_id)
            .expect("existing current-profile row");
        fs::create_dir_all(file_path(row).parent().unwrap()).unwrap();
        fs::write(file_path(row), &bytes).unwrap();
        row.length = bytes.len();
        row.hash = hash(&bytes);
    }
    let mut output = String::from(HEADER);
    for row in rows {
        output.push_str(&format!(
            "{},{},{},{},{}\n",
            row.profile, row.case_id, row.path, row.length, row.hash
        ));
    }
    fs::write(path, output).unwrap();
}
