use std::{path::PathBuf, process::Command};

#[test]
fn frontend_contract() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/00_retained_item.rs");
    cases.pass("tests/ui/pass/01_hygiene_generics.rs");
    cases.pass("tests/ui/pass/02_tagged_payload_root.rs");
    cases.pass("tests/ui/pass/03_hygiene_visibility_raw_paths.rs");
    cases.pass("tests/ui/pass/04_access_copy_mutation.rs");
    cases.pass("tests/ui/pass/05_retained_literals_patterns.rs");
    cases.pass("tests/ui/pass/06_shadowed_prelude.rs");
    cases.pass("tests/ui/pass/07_raw_tagged_keyword.rs");
    cases.pass("tests/ui/pass/08_optional_zero_sentinel.rs");
    cases.pass("tests/ui/pass/09_option_capability_surface.rs");
    cases.pass("tests/ui/pass/10_optional_source_lifetime.rs");
    cases.pass("tests/ui/pass/11_symbolic_nonzero_arrays.rs");

    cases.compile_fail("tests/ui/fail/00_removed_derive.rs");
    cases.compile_fail("tests/ui/fail/01_removed_container_options.rs");
    cases.compile_fail("tests/ui/fail/02_removed_field_options.rs");
    cases.compile_fail("tests/ui/fail/03_removed_api.rs");
    cases.compile_fail("tests/ui/fail/01_exact_grammar.rs");
    cases.compile_fail("tests/ui/fail/02_array_expression.rs");
    cases.compile_fail("tests/ui/fail/03_lifetime_ambiguity.rs");
    cases.compile_fail("tests/ui/fail/04_tag_field_syntax.rs");
    cases.compile_fail("tests/ui/fail/05_generated_collision.rs");
    cases.compile_fail("tests/ui/fail/06_unsupported_item.rs");
    cases.compile_fail("tests/ui/fail/06_tag_field_non_enum.rs");
    cases.compile_fail("tests/ui/fail/07_tag_field_missing_value.rs");
    cases.compile_fail("tests/ui/fail/08_tag_field_shared.rs");
    cases.compile_fail("tests/ui/fail/09_tagged_root_api.rs");
    cases.compile_fail("tests/ui/fail/10_tagged_root_buffer.rs");
    cases.compile_fail("tests/ui/fail/11_zero_length_array.rs");
    cases.compile_fail("tests/ui/fail/12_zero_sized_root.rs");
    cases.compile_fail("tests/ui/fail/13_zero_sized_member.rs");
    cases.compile_fail("tests/ui/fail/14_unsupported_array_element.rs");
    cases.compile_fail("tests/ui/fail/15_recursive_array.rs");
    cases.compile_fail("tests/ui/fail/16_u16cstr_endian.rs");
    cases.compile_fail("tests/ui/fail/18_collision_access_mut.rs");
    cases.compile_fail("tests/ui/fail/19_collision_schema_size.rs");
    cases.compile_fail("tests/ui/fail/20_collision_schema_align.rs");
    cases.compile_fail("tests/ui/fail/21_collision_schema_stride.rs");
    cases.compile_fail("tests/ui/fail/22_collision_layout.rs");
    cases.compile_fail("tests/ui/fail/23_collision_copy_into.rs");
    cases.compile_fail("tests/ui/fail/24_collision_copy_from.rs");
    cases.compile_fail("tests/ui/fail/25_collision_field_methods.rs");
    cases.compile_fail("tests/ui/fail/26_collision_variant_methods.rs");
    cases.compile_fail("tests/ui/fail/27_collision_generated_types.rs");
    cases.compile_fail("tests/ui/fail/28_hidden_wire_and_legacy_api.rs");
    cases.compile_fail("tests/ui/fail/29_scalar_variant_root_collision.rs");
    cases.compile_fail("tests/ui/fail/30_tagged_variant_fixed_collision.rs");
    cases.compile_fail("tests/ui/fail/31_hidden_proof_bypass.rs");
    cases.compile_fail("tests/ui/fail/32_hidden_proof_literal.rs");
    cases.compile_fail("tests/ui/fail/33_hidden_tag_selection_bypass.rs");
    cases.compile_fail("tests/ui/fail/34_root_shared_input_token.rs");
    cases.compile_fail("tests/ui/fail/35_root_exclusive_input_token.rs");
    cases.compile_fail("tests/ui/fail/36_direct_commit_token.rs");
    cases.compile_fail("tests/ui/fail/37_root_token_coherence.rs");
    cases.compile_fail("tests/ui/fail/38_sibling_token_getter.rs");
    cases.compile_fail("tests/ui/fail/39_hidden_wire_physical_access.rs");
    cases.compile_fail("tests/ui/fail/40_hidden_wire_copy.rs");
    cases.compile_fail("tests/ui/fail/41_optional_primitive.rs");
    cases.compile_fail("tests/ui/fail/42_optional_zero_valid.rs");
    cases.compile_fail("tests/ui/fail/43_optional_nested.rs");
    cases.compile_fail("tests/ui/fail/44_optional_array_element.rs");
    cases.compile_fail("tests/ui/fail/45_optional_forbidden_attribute.rs");
    cases.compile_fail("tests/ui/fail/46_optional_tagged_payload.rs");
    cases.compile_fail("tests/ui/fail/47_optional_bool.rs");
    cases.compile_fail("tests/ui/fail/48_optional_option_grammar.rs");
    cases.compile_fail("tests/ui/fail/49_optional_primitive_array.rs");
    cases.compile_fail("tests/ui/fail/50_private_option_initialization.rs");
    cases.compile_fail("tests/ui/fail/51_optional_wire_and_legacy_surface.rs");
    cases.compile_fail("tests/ui/fail/52_option_mut_borrow_blocks_set.rs");
    cases.compile_fail("tests/ui/fail/53_generated_token_literal.rs");
    cases.compile_fail("tests/ui/fail/54_option_mut_extraction.rs");
    cases.compile_fail("tests/ui/fail/55_option_mut_literal.rs");
    cases.compile_fail("tests/ui/fail/56_optional_canonical_primitives.rs");
    cases.compile_fail("tests/ui/fail/57_optional_borrowed.rs");
    cases.compile_fail("tests/ui/fail/58_optional_direct_tagged.rs");
    cases.compile_fail("tests/ui/fail/59_optional_zero_valid_record.rs");
    cases.compile_fail("tests/ui/fail/60_symbolic_zero_arrays.rs");
    cases.compile_fail("tests/ui/fail/61_optional_std_primitive.rs");
    cases.compile_fail("tests/ui/fail/62_optional_std_bool_array.rs");
    cases.compile_fail("tests/ui/fail/63_optional_c_str.rs");
    cases.compile_fail("tests/ui/fail/64_optional_u16_str.rs");
    cases.compile_fail("tests/ui/fail/65_optional_u16_c_str.rs");
    cases.compile_fail("tests/ui/fail/66_optional_fixed_bytes.rs");
    cases.compile_fail("tests/ui/fail/67_optional_tagged_array.rs");
    cases.compile_fail("tests/ui/fail/68_optional_zero_length_array.rs");
    cases.compile_fail("tests/ui/fail/69_optional_zero_valid_record_array.rs");
    cases.compile_fail("tests/ui/fail/70_optional_zero_valid_scalar_array.rs");
    cases.compile_fail("tests/ui/fail/71_nested_symbolic_zero_array.rs");
    cases.compile_fail("tests/ui/fail/72_optional_zero_valid_tagged_variant.rs");
}

#[test]
fn isolated_frontend_fixtures_are_targeted_and_warning_free() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (fixture, diagnostic) in [
        ("local-item-fail", "no `Local` in the root"),
        (
            "missing-zerocopy-fail",
            "requires the consuming crate to depend directly on zerocopy",
        ),
        (
            "missing-tag-field-fail",
            "the trait bound `Payload: WireType` is not satisfied",
        ),
        (
            "wrong-tag-enum-fail",
            "ActualKind: __zero_schema_same_type<ExpectedKind>",
        ),
        ("legacy-naming-surface-fail", concat!("copy_from", "to")),
    ] {
        let manifest = root.join(format!("tests/fixtures/{fixture}/Cargo.toml"));
        let output = Command::new("cargo")
            .args(["+1.85.0", "check", "--locked", "--manifest-path"])
            .arg(manifest)
            .arg("--target-dir")
            .arg(root.join(format!("../target/ui-fixtures/{fixture}")))
            .output()
            .expect("run isolated failing frontend fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{fixture} unexpectedly compiled");
        assert!(
            stderr.contains(diagnostic),
            "{fixture} did not report `{diagnostic}`:\n{stderr}"
        );
        if fixture == "legacy-naming-surface-fail" {
            for (removed, diagnostic) in [
                (
                    concat!("copy_from", "to"),
                    concat!("no method named `", "copy_from", "to", "` found"),
                ),
                (
                    concat!("Array", "View"),
                    concat!("no `", "Array", "View", "` in the root"),
                ),
                (
                    concat!("Array", "Iter"),
                    concat!("no `", "Array", "Iter", "` in the root"),
                ),
                (
                    concat!("Payload", "Wire"),
                    concat!("cannot find type `", "Payload", "Wire", "` in crate `zs`"),
                ),
                (
                    concat!("Tagged", "Selection"),
                    concat!(
                        "cannot find type `",
                        "Tagged",
                        "Selection",
                        "` in crate `zs`"
                    ),
                ),
            ] {
                assert!(
                    stderr.contains(diagnostic),
                    "{fixture} did not reject removed `{removed}` with `{diagnostic}`:\n{stderr}"
                );
            }
        }
    }

    for fixture in [
        "aggregate-pass",
        "criterion-import-pass",
        "renamed-dependencies-pass",
    ] {
        let manifest = root.join(format!("tests/fixtures/{fixture}/Cargo.toml"));
        let output = Command::new("cargo")
            .args(["+1.85.0", "check", "--locked", "--manifest-path"])
            .arg(manifest)
            .arg("--target-dir")
            .arg(root.join(format!("../target/ui-fixtures/{fixture}")))
            .output()
            .expect("run isolated passing frontend fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{fixture} failed:\n{stderr}");
        assert!(
            !stderr.contains("warning:"),
            "{fixture} emitted a warning:\n{stderr}"
        );
    }
}

#[test]
fn cross_crate_generated_projections_remain_opaque() {
    let macro_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = macro_root
        .parent()
        .expect("macro crate has repository parent");
    let output = Command::new("cargo")
        .current_dir(repository_root)
        .args([
            "+1.85.0",
            "test",
            "--locked",
            "-p",
            "zero-schema-cross-crate-consumer",
            "--lib",
        ])
        .arg("--target-dir")
        .arg(macro_root.join("../target/ui-fixtures/cross-crate-consumer"))
        .output()
        .expect("compile cross-crate generated consumer");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cross-crate opaque projection fixture failed:\n{stderr}"
    );
    assert!(
        !stderr.contains("warning:"),
        "cross-crate opaque projection fixture emitted a warning:\n{stderr}"
    );
}
