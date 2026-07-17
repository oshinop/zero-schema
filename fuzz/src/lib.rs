#![forbid(unsafe_code)]

use core::hint::black_box;
use zero_schema::zero;

use zero_schema_schema_corpus::{
    AllFeatures, AllFeaturesPatch, CorpusCode8, CorpusCode8Patch, CorpusCode16Be,
    CorpusCode16BePatch, ExternalCorpusMessage, ExternalCorpusMessagePatch, FuzzAllStrings,
    FuzzAllStringsPatch,
};

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionalFuzzCode {
    One = 1,
    Two = 2,
}

#[zero]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalFuzzChild {
    code: OptionalFuzzCode,
    payload: u16,
}

#[zero]
pub struct OptionalFuzzRoot {
    before: u8,
    #[zero(align = 8)]
    maybe_code: Option<OptionalFuzzCode>,
    maybe_child: Option<OptionalFuzzChild>,
    maybe_codes: Option<[OptionalFuzzCode; 2]>,
    after: u8,
}

pub const TARGET_COUNTS: &[(&str, usize)] = &[
    ("parse_message", 3),
    ("parse_external_tag", 1),
    ("parse_all_strings", 1),
    ("roundtrip_message", 1),
];

/// Fills an exactly-sized, schema-aligned receiving buffer without assigning
/// any schema interpretation to the bytes. Repetition makes every input byte
/// influence a bounded scratch buffer while retaining deterministic work.
fn receive(destination: &mut [u8], input: &[u8]) {
    if input.is_empty() {
        destination.fill(0);
        return;
    }

    for (index, byte) in destination.iter_mut().enumerate() {
        *byte = input[index % input.len()];
    }
}

fn selector(input: &[u8], count: usize) -> (usize, &[u8]) {
    let Some((&selector, payload)) = input.split_first() else {
        return (0, &[]);
    };
    (usize::from(selector) % count, payload)
}

fn duplicate(source: &[u8], destination: &mut [u8]) -> bool {
    if source.len() != destination.len() {
        return false;
    }
    destination.copy_from_slice(source);
    true
}

fn release<T>(value: T) {
    drop(value);
}

fn exercise_code8(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(CorpusCode8);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = CorpusCode8::access(source.as_bytes()) else {
        return false;
    };
    let logical = view.copy_into();
    black_box(view.get());
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(CorpusCode8);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = CorpusCode8::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view.copy_from(&CorpusCode8Patch::default()).is_err() {
        return false;
    }
    if view.copy_from(&CorpusCode8Patch::from(logical)).is_err() {
        return false;
    }
    release(view);

    let Ok(view) = CorpusCode8::access(destination.as_bytes()) else {
        return false;
    };
    black_box(view.copy_into());
    true
}

fn exercise_code16(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(CorpusCode16Be);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = CorpusCode16Be::access(source.as_bytes()) else {
        return false;
    };
    let logical = view.copy_into();
    black_box(view.get());
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(CorpusCode16Be);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = CorpusCode16Be::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view.copy_from(&CorpusCode16BePatch::default()).is_err() {
        return false;
    }
    if view.copy_from(&CorpusCode16BePatch::from(logical)).is_err() {
        return false;
    }
    release(view);

    let Ok(view) = CorpusCode16Be::access(destination.as_bytes()) else {
        return false;
    };
    black_box(view.copy_into());
    true
}

fn exercise_strings(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(FuzzAllStrings<'static>);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = FuzzAllStrings::access(source.as_bytes()) else {
        return false;
    };
    let text = view.text();
    let c_text = view.c_text();
    let wide = view.wide();
    let wide_c = view.wide_c();
    black_box(text);
    black_box(c_text.to_bytes());
    black_box(wide.as_slice());
    black_box(wide_c.as_slice());
    let logical = view.copy_into();
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(FuzzAllStrings<'static>);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = FuzzAllStrings::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view.copy_from(&FuzzAllStringsPatch::default()).is_err() {
        return false;
    }
    if view.text_mut().set(text).is_err()
        || view.c_text_mut().set(c_text).is_err()
        || view.wide_mut().set(wide).is_err()
        || view.wide_c_mut().set(wide_c).is_err()
    {
        return false;
    }
    if view.copy_from(&FuzzAllStringsPatch::from(logical)).is_err() {
        return false;
    }
    release(view);

    let Ok(view) = FuzzAllStrings::access(destination.as_bytes()) else {
        return false;
    };
    black_box(view.copy_into());
    true
}

fn exercise_external_union(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(ExternalCorpusMessage);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = ExternalCorpusMessage::access(source.as_bytes()) else {
        return false;
    };
    black_box(view.tag());
    let payload = view.payload();
    black_box(payload.tag());
    black_box(payload.unit());
    if let Some(value) = payload.payload() {
        black_box(value.value());
    }
    black_box(payload.copy_into());
    let logical = view.copy_into();
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(ExternalCorpusMessage);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = ExternalCorpusMessage::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view
        .copy_from(&ExternalCorpusMessagePatch::default())
        .is_err()
    {
        return false;
    }
    if view
        .copy_from(&ExternalCorpusMessagePatch::from(logical))
        .is_err()
    {
        return false;
    }
    release(view);

    let Ok(view) = ExternalCorpusMessage::access(destination.as_bytes()) else {
        return false;
    };
    let payload = view.payload();
    black_box(payload.tag());
    black_box(payload.unit());
    if let Some(value) = payload.payload() {
        black_box(value.value());
    }
    black_box(view.copy_into());
    true
}

fn exercise_record(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(AllFeatures<'static>);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = AllFeatures::access(source.as_bytes()) else {
        return false;
    };

    let sequence = view.sequence();
    let active = view.active();
    let priority = view.priority();
    let name = view.name();
    let c_name = view.c_name();
    let wide = view.wide();
    let wide_c = view.wide_c();
    let token = view.token();
    black_box(sequence);
    black_box(active);
    black_box(priority);
    black_box(name);
    black_box(c_name.to_bytes());
    black_box(wide.as_slice());
    black_box(wide_c.as_slice());
    black_box(token);

    let header = view.header();
    let header_version = header.version();
    let header_producer = header.producer();
    black_box(header_version);
    black_box(header_producer.to_bytes());

    let samples = view.samples();
    black_box(samples.get(0));
    for value in samples.iter() {
        black_box(value);
    }
    let sample_values = samples.copy_into();
    black_box(sample_values);

    let headers = view.headers();
    black_box(headers.get(0).map(|header| header.version()));
    for header in headers.iter() {
        black_box(header.version());
        black_box(header.producer().to_bytes());
    }
    black_box(headers.copy_into());

    black_box(view.config_kind());
    let config = view.config();
    black_box(config.tag());
    if let Some(file) = config.file() {
        black_box(file.header().version());
        black_box(file.header().producer().to_bytes());
        black_box(file.flags());
    }
    if let Some(memory) = config.memory() {
        black_box(memory.capacity());
        black_box(memory.enabled());
    }
    black_box(config.copy_into());
    let checksum = view.checksum();
    black_box(checksum);
    let logical = view.copy_into();
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(AllFeatures<'static>);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = AllFeatures::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view.copy_from(&AllFeaturesPatch::default()).is_err() {
        return false;
    }

    if view.sequence_mut().set(sequence).is_err()
        || view.active_mut().set(active).is_err()
        || view.priority_mut().set(priority).is_err()
        || view.name_mut().set(name).is_err()
        || view.c_name_mut().set(c_name).is_err()
        || view.wide_mut().set(wide).is_err()
        || view.wide_c_mut().set(wide_c).is_err()
        || view.token_mut().set(token).is_err()
        || view.checksum_mut().set(checksum).is_err()
    {
        return false;
    }
    {
        let mut header = view.header_mut();
        if header.version_mut().set(header_version).is_err()
            || header.producer_mut().set(header_producer).is_err()
        {
            return false;
        }
    }
    {
        let mut samples = view.samples_mut();
        if samples.copy_from(&sample_values).is_err() || samples.get_mut(0).is_none() {
            return false;
        }
    }
    {
        let mut headers = view.headers_mut();
        if headers.copy_from(&logical.headers).is_err() || headers.get_mut(0).is_none() {
            return false;
        }
    }

    let patch = AllFeaturesPatch::from(logical);
    if view.copy_from(&patch).is_err() {
        return false;
    }
    {
        let Some(config_patch) = patch.config.as_ref() else {
            return false;
        };
        let mut config = view.config_mut();
        if config.copy_from(config_patch).is_err() {
            return false;
        }
    }
    release(view);

    let Ok(view) = AllFeatures::access(destination.as_bytes()) else {
        return false;
    };
    black_box(view.samples().copy_into());
    black_box(view.headers().copy_into());
    black_box(view.config().copy_into());
    black_box(view.copy_into());
    true
}

/// Exercises a local zero-sentinel schema from every existing fuzz target
/// without changing the registered deterministic corpus inventory.
fn exercise_optional(input: &[u8]) -> bool {
    let mut source = zero_schema::make_schema_buffer!(OptionalFuzzRoot);
    receive(source.as_bytes_mut(), input);
    let Ok(view) = OptionalFuzzRoot::access(source.as_bytes()) else {
        return false;
    };
    black_box(view.maybe_code());
    black_box(view.maybe_child().map(|child| child.payload()));
    black_box(view.maybe_codes().map(|codes| codes.copy_into()));
    let logical = view.copy_into();
    black_box(&logical);

    let mut destination = zero_schema::make_schema_buffer!(OptionalFuzzRoot);
    if !duplicate(source.as_bytes(), destination.as_bytes_mut()) {
        return false;
    }
    let Ok(mut view) = OptionalFuzzRoot::access_mut(destination.as_bytes_mut()) else {
        return false;
    };
    if view.copy_from(&OptionalFuzzRootPatch::default()).is_err() {
        return false;
    }
    if view
        .copy_from(&OptionalFuzzRootPatch::from(logical))
        .is_err()
    {
        return false;
    }
    release(view);

    let Ok(view) = OptionalFuzzRoot::access(destination.as_bytes()) else {
        return false;
    };
    black_box(view.copy_into());
    true
}

fn dispatch_accepts(target: &str, input: &[u8]) -> bool {
    black_box(exercise_optional(input));
    match target {
        "parse_message" => match selector(input, 3) {
            (0, payload) => exercise_code16(payload),
            (1, payload) => exercise_external_union(payload),
            (2, payload) => exercise_code8(payload),
            _ => false,
        },
        "parse_external_tag" => exercise_external_union(input),
        "parse_all_strings" => exercise_strings(input),
        "roundtrip_message" => exercise_record(input),
        _ => false,
    }
}

pub fn parse_message(input: &[u8]) {
    black_box(dispatch_accepts("parse_message", input));
}

pub fn parse_external_tag(input: &[u8]) {
    black_box(dispatch_accepts("parse_external_tag", input));
}

pub fn parse_all_strings(input: &[u8]) {
    black_box(dispatch_accepts("parse_all_strings", input));
}

pub fn roundtrip_message(input: &[u8]) {
    black_box(dispatch_accepts("roundtrip_message", input));
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
    use zero_schema::SchemaError;

    const HEADER: &str = "root_id,type_key,fuzz_target,selector,golden_path,golden_len,golden_sha256,valid_seed_path,valid_seed_sha256,invalid_seed_path,invalid_seed_sha256";

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("fuzz crate has a workspace parent")
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
                && !relative.split('/').any(|part| part == "..")
        );
        let bytes = fs::read(root.join(relative)).expect("registered fixture exists");
        assert_eq!(
            hash(&bytes),
            expected_hash,
            "fixture hash drifted: {relative}"
        );
        bytes
    }

    fn fuzz_binary_names(manifest: &str) -> Vec<&str> {
        manifest
            .split("[[bin]]")
            .skip(1)
            .map(|section| {
                section
                    .lines()
                    .find_map(|line| {
                        line.trim()
                            .strip_prefix("name = \"")
                            .and_then(|name| name.strip_suffix('"'))
                    })
                    .expect("every fuzz binary has a name")
            })
            .collect()
    }

    fn fuzz_target_entrypoint(root: &Path, target: &str) -> String {
        let source =
            fs::read_to_string(root.join("fuzz/fuzz_targets").join(format!("{target}.rs")))
                .expect("registered fuzz target source exists");
        source
            .split_once("zero_schema_fuzz::")
            .and_then(|(_, call)| call.split_once('('))
            .map(|(entrypoint, _)| entrypoint.to_owned())
            .expect("fuzz target calls a zero-schema-fuzz entrypoint")
    }

    #[test]
    fn inventory_hashes_registered_producer_seeds_and_target_selection_are_strict() {
        let root = workspace();
        let manifest =
            fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz manifest exists");
        let registered_targets: BTreeSet<_> =
            TARGET_COUNTS.iter().map(|&(target, _)| target).collect();
        let binary_names = fuzz_binary_names(&manifest);
        assert_eq!(binary_names.len(), TARGET_COUNTS.len());
        assert_eq!(
            binary_names.into_iter().collect::<BTreeSet<_>>(),
            registered_targets
        );
        let text = fs::read_to_string(root.join("test-fixtures/schema-corpus/inventory.csv"))
            .expect("schema corpus inventory exists");
        assert!(!text.contains('\r') && text.ends_with('\n'));
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(HEADER));
        let mut selectors: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
        let mut roots = BTreeSet::new();
        let mut registrations: BTreeSet<(&str, &str, u8)> = BTreeSet::new();

        for line in lines {
            assert!(!line.is_empty() && !line.starts_with('#') && !line.contains('"'));
            let columns: Vec<_> = line.split(',').collect();
            assert_eq!(columns.len(), 11);
            let root_id: u32 = columns[0].parse().expect("root id is numeric");
            assert!(roots.insert(root_id), "root id is unique");
            let target = columns[2];
            let selector: usize = columns[3].parse().expect("selector is numeric");
            assert!((1..=256).contains(&selector));
            selectors.entry(target).or_default().insert(selector);
            registrations.insert((
                columns[0],
                target,
                u8::try_from(selector).expect("selector is representable in corpus metadata"),
            ));

            let golden = checked_file(&root, columns[4], columns[6]);
            assert_eq!(
                golden.len(),
                columns[5]
                    .parse::<usize>()
                    .expect("golden length is numeric")
            );
            assert_eq!(
                columns[4],
                format!("test-fixtures/schema-corpus/golden/{root_id}.bin")
            );
            for digest in [columns[6], columns[8], columns[10]] {
                assert_eq!(digest.len(), 64);
                assert!(
                    digest
                        .bytes()
                        .all(|byte| { byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) })
                );
            }
            assert_eq!(columns[7], format!("fuzz/corpus/{target}/{selector}-valid"));
            assert_eq!(
                columns[9],
                format!("fuzz/corpus/{target}/{selector}-invalid")
            );

            let valid = checked_file(&root, columns[7], columns[8]);
            let invalid = checked_file(&root, columns[9], columns[10]);
            if target == "parse_message" {
                assert_eq!(valid.first(), Some(&((selector - 1) as u8)));
                assert_eq!(&valid[1..], golden.as_slice());
                assert_eq!(invalid.first(), Some(&((selector - 1) as u8)));
            } else {
                assert_eq!(valid, golden);
            }
            assert!(
                dispatch_accepts(target, &valid),
                "valid producer seed rejected: {target}/{selector}"
            );
            assert!(
                !dispatch_accepts(target, &invalid),
                "invalid seed accepted: {target}/{selector}"
            );
        }

        for &(target, count) in TARGET_COUNTS {
            assert_eq!(
                selectors.remove(target),
                Some((1..=count).collect()),
                "inventory selector registration drifted for {target}"
            );
        }
        assert!(
            selectors.is_empty(),
            "inventory names an unknown fuzz target"
        );
        assert_eq!(roots.len(), zero_schema_schema_corpus::ROOT_IDS.len());
        let expected_registrations: BTreeSet<_> = zero_schema_schema_corpus::FUZZ_TARGETS
            .iter()
            .copied()
            .collect();
        assert_eq!(registrations, expected_registrations);
    }

    #[test]
    fn fuzz_target_sources_dispatch_to_matching_library_entrypoints() {
        let root = workspace();
        for &(target, _) in TARGET_COUNTS {
            assert_eq!(
                fuzz_target_entrypoint(&root, target),
                target,
                "fuzz target `{target}` dispatches to a different library entrypoint"
            );
        }
    }

    #[test]
    fn arbitrary_initialized_inputs_never_panic() {
        let payloads: &[&[u8]] = &[
            &[],
            &[0],
            &[0xff],
            &[0, 1, 2, 3, 4, 5, 6, 7, 8],
            &[2, 0xff, 0, 0, 1, 0xff, 0, 0, 0, 0, 0, 0],
        ];
        let targets: &[fn(&[u8])] = &[
            parse_message,
            parse_external_tag,
            parse_all_strings,
            roundtrip_message,
        ];
        for target in targets {
            for payload in payloads {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| target(payload)));
                assert!(
                    result.is_ok(),
                    "fuzz target panicked for arbitrary initialized input"
                );
            }
        }
    }

    #[test]
    fn reviewed_all_features_producer_bytes_complete_access_materialize_patch_and_reaccess() {
        let bytes = include_bytes!("../../test-fixtures/schema-corpus/golden/6.bin");
        assert_eq!(bytes.len(), AllFeatures::SCHEMA_SIZE);
        assert!(exercise_record(bytes));
    }

    #[test]
    fn option_schema_parses_mutates_roundtrips_and_clears_exactly() {
        #[repr(align(8))]
        struct AlignedBytes([u8; OptionalFuzzRoot::SCHEMA_SIZE]);

        let field = |name| {
            OptionalFuzzRoot::LAYOUT
                .fields()
                .iter()
                .find(|field| field.name() == name)
                .expect("declared optional field")
        };
        let mut bytes = AlignedBytes([0; OptionalFuzzRoot::SCHEMA_SIZE]);
        let absent = OptionalFuzzRoot::access(&bytes.0)
            .expect("all-zero optional spans are absent")
            .copy_into();
        assert_eq!(absent.maybe_code, None);
        assert_eq!(absent.maybe_child, None);
        assert_eq!(absent.maybe_codes, None);
        assert!(
            exercise_optional(&bytes.0),
            "all-zero optionals roundtrip through targets"
        );

        {
            let mut root = OptionalFuzzRoot::access_mut(&mut bytes.0)
                .expect("all-zero optional spans are mutable");
            root.before_mut().set(0x31).expect("set preceding field");
            root.after_mut().set(0x53).expect("set following field");
            root.maybe_code_mut()
                .set(Some(OptionalFuzzCode::One))
                .expect("initialize optional enum");
            root.maybe_child_mut()
                .set(Some(OptionalFuzzChild {
                    code: OptionalFuzzCode::Two,
                    payload: 23,
                }))
                .expect("initialize optional child");
            root.maybe_codes_mut()
                .set(Some([OptionalFuzzCode::One, OptionalFuzzCode::Two]))
                .expect("initialize optional enum array");
        }
        let present = OptionalFuzzRoot::access(&bytes.0)
            .expect("initialized optionals are valid")
            .copy_into();
        assert_eq!(present.maybe_code, Some(OptionalFuzzCode::One));
        assert_eq!(
            present.maybe_child,
            Some(OptionalFuzzChild {
                code: OptionalFuzzCode::Two,
                payload: 23,
            })
        );
        assert_eq!(
            present.maybe_codes,
            Some([OptionalFuzzCode::One, OptionalFuzzCode::Two])
        );
        assert!(exercise_optional(&bytes.0));

        let code = field("maybe_code");
        assert!(code.size() > 1, "aligned option includes storage padding");
        let mut malformed = AlignedBytes([0; OptionalFuzzRoot::SCHEMA_SIZE]);
        malformed.0[code.offset() + code.size() - 1] = 0x44;
        assert!(
            OptionalFuzzRoot::access(&malformed.0).is_err(),
            "nonzero option padding requires proving the invalid all-zero enum"
        );
        assert!(
            !exercise_optional(&malformed.0),
            "fuzz roundtrip rejects malformed nonzero optional storage"
        );

        let mut absent = AlignedBytes([0; OptionalFuzzRoot::SCHEMA_SIZE]);
        let before_incomplete = absent.0;
        let incomplete = OptionalFuzzRootPatch {
            maybe_child: Some(Some(OptionalFuzzChildPatch {
                code: None,
                payload: Some(99),
            })),
            ..Default::default()
        };
        let error = OptionalFuzzRoot::access_mut(&mut absent.0)
            .expect("absent optional child is mutable")
            .copy_from(&incomplete)
            .expect_err("partial patch cannot initialize an absent optional child");
        assert_eq!(
            error.kind(),
            zero_schema::ErrorKind::IncompleteOptionalInitialization
        );
        assert_eq!(absent.0, before_incomplete, "failed patch is byte-exact");

        let child = field("maybe_child");
        let mut clear_bytes = AlignedBytes([0; OptionalFuzzRoot::SCHEMA_SIZE]);
        {
            let mut root = OptionalFuzzRoot::access_mut(&mut clear_bytes.0)
                .expect("absent optional child is mutable");
            root.before_mut().set(0x31).expect("set preceding field");
            root.after_mut().set(0x53).expect("set following field");
            root.maybe_child_mut()
                .set(Some(OptionalFuzzChild {
                    code: OptionalFuzzCode::One,
                    payload: 41,
                }))
                .expect("initialize child for exact clear");
        }
        clear_bytes.0[child.offset() + 1] = 0xc1;
        assert!(
            OptionalFuzzRoot::access(&clear_bytes.0).is_ok(),
            "nonzero internal child padding stays valid while the child is Some"
        );
        let before_clear = clear_bytes.0;
        let clear = OptionalFuzzRootPatch {
            maybe_child: Some(None),
            ..Default::default()
        };
        OptionalFuzzRoot::access_mut(&mut clear_bytes.0)
            .expect("present child is mutable")
            .copy_from(&clear)
            .expect("Some(None) clears the optional child");
        assert!(
            clear_bytes.0[child.offset()..child.offset() + child.size()]
                .iter()
                .all(|byte| *byte == 0),
            "patch clear zeros the complete optional storage span"
        );
        for (index, byte) in clear_bytes.0.iter().enumerate() {
            if index < child.offset() || index >= child.offset() + child.size() {
                assert_eq!(*byte, before_clear[index], "clear touched byte {index}");
            }
        }
    }
}
