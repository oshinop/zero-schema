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
        assert!(
            columns.iter().all(|value| !value.contains('"')),
            "CSV quoting forbidden"
        );
        let case_id = columns[1].parse::<u32>().expect("decimal case ID");
        let length = columns[3].parse::<usize>().expect("decimal length");
        assert!(case_id != 0 && length != 0);
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
    assert_eq!(rows.len(), 55);
    let mut identities = BTreeSet::new();
    let mut declared_files = BTreeSet::new();
    for (profile_index, profile) in PROFILES.iter().enumerate() {
        for case_index in 0..11 {
            let row = &rows[profile_index * 11 + case_index];
            assert_eq!(&row.profile, profile);
            assert_eq!(row.case_id, 1001 + case_index as u32);
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
            let path = entry.unwrap().path();
            assert!(path.is_file());
            actual_files.insert(path);
        }
    }
    assert_eq!(actual_files, declared_files);
}

#[test]
fn current_profile_matches_both_implementations() {
    let rows = parse_manifest(&fs::read(manifest_path()).expect("golden manifest"));
    let current: BTreeMap<_, _> = rows
        .iter()
        .filter(|row| row.profile == BUILD_PROFILE)
        .map(|row| (row.case_id, row))
        .collect();
    assert_eq!(current.len(), 11, "current build profile must be reviewed");
    for case in CASES {
        let rust = (case.rust_bytes)().unwrap();
        let cpp = cpp_write_fixture(case.case_id, rust.len()).unwrap();
        assert_eq!(cpp, rust);
        assert_eq!(fs::read(file_path(current[&case.case_id])).unwrap(), rust);
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn reviewed_cross_profile_invariants_hold() {
    let rows = parse_manifest(&fs::read(manifest_path()).expect("golden manifest"));
    let lookup: BTreeMap<_, _> = rows
        .iter()
        .map(|row| {
            (
                (row.profile.as_str(), row.case_id),
                fs::read(file_path(row)).unwrap(),
            )
        })
        .collect();
    let reference = &lookup[&((PROFILES[0]), 1001)];
    for profile in PROFILES {
        assert_eq!(
            &lookup[&(profile, 1001)],
            reference,
            "case 1001 is target-independent"
        );
    }
    let little_message = &lookup[&("linux-x86_64-le", 1004)];
    for profile in [
        "linux-x86_64-le",
        "linux-i686-le",
        "macos-aarch64-le",
        "windows-x86_64-msvc-le",
    ] {
        assert_eq!(
            &lookup[&(profile, 1004)],
            little_message,
            "case 1004 little-endian layouts agree"
        );
    }
    let mut reversed_message = little_message.clone();
    reversed_message[4..8].reverse();
    assert_eq!(
        &lookup[&("linux-powerpc64-be", 1004)],
        &reversed_message,
        "only case 1004 native u32 reverses"
    );

    for profile in PROFILES {
        let bytes = &lookup[&(profile, 1005)];
        assert_eq!(bytes[0], 0xa5, "one-byte marker is stable");
        assert!(
            contains(bytes, &[0x02, 0x01]) && contains(bytes, &[0x01, 0x02]),
            "u16 LE/BE direction"
        );
        assert!(
            contains(bytes, &[0x02, 0x81]) && contains(bytes, &[0x81, 0x02]),
            "i16 LE/BE direction"
        );
        assert!(
            contains(bytes, &[4, 3, 2, 1]) && contains(bytes, &[1, 2, 3, 4]),
            "u32 LE/BE direction"
        );
        assert!(
            contains(bytes, &[4, 3, 2, 0x81]) && contains(bytes, &[0x81, 2, 3, 4]),
            "i32 LE/BE direction"
        );
        assert!(
            contains(bytes, &[8, 7, 6, 5, 4, 3, 2, 1])
                && contains(bytes, &[1, 2, 3, 4, 5, 6, 7, 8]),
            "u64 LE/BE direction"
        );
        assert!(
            contains(bytes, &[8, 7, 6, 5, 4, 3, 2, 0x81])
                && contains(bytes, &[0x81, 2, 3, 4, 5, 6, 7, 8]),
            "i64 LE/BE direction"
        );
        assert!(
            contains(bytes, &[0x34, 0x12, 0xc0, 0x7f])
                && contains(bytes, &[0x7f, 0xc0, 0x12, 0x34]),
            "f32 LE/BE direction"
        );
        assert!(
            contains(bytes, &[0x34, 0x12, 0, 0, 0, 0, 0xf8, 0x7f])
                && contains(bytes, &[0x7f, 0xf8, 0, 0, 0, 0, 0x12, 0x34]),
            "f64 LE/BE direction"
        );
    }
    let little = &lookup[&("linux-x86_64-le", 1005)];
    let big = &lookup[&("linux-powerpc64-be", 1005)];
    assert!(
        contains(little, &[0x22, 0x11]) && contains(big, &[0x11, 0x22]),
        "native u16 reverses cross-endian"
    );
    assert!(
        contains(little, &[0x44, 0x33, 0x22, 0x11]) && contains(big, &[0x11, 0x22, 0x33, 0x44]),
        "native u32 reverses cross-endian"
    );
    assert!(
        contains(little, &[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11])
            && contains(big, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]),
        "native u64 reverses cross-endian"
    );
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
        let rust = (case.rust_bytes)().unwrap();
        let cpp = cpp_write_fixture(case.case_id, rust.len()).unwrap();
        assert_eq!(cpp, rust, "never publish divergent implementations");
        let row = rows
            .iter_mut()
            .find(|row| row.profile == BUILD_PROFILE && row.case_id == case.case_id)
            .expect("existing current-profile row");
        fs::create_dir_all(file_path(row).parent().unwrap()).unwrap();
        fs::write(file_path(row), &rust).unwrap();
        row.length = rust.len();
        row.hash = hash(&rust);
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
