use std::path::PathBuf;
use std::process::Command;

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/00_scalar_only.rs");
    for case in 1..=14 {
        t.compile_fail(format!("tests/ui/fail/{case:02}_*.rs"));
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (fixture, should_pass) in [
        ("aggregate-pass", true),
        ("struct-hygiene-pass", true),
        ("local-item-fail", false),
        ("wide-target-fail", false),
    ] {
        let manifest = root.join(format!("tests/fixtures/{fixture}/Cargo.toml"));
        let target = root.join(format!("../target/ui-fixtures/{fixture}"));
        let output = Command::new("cargo")
            .args(["+1.85.0", "check", "--locked", "--manifest-path"])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .output()
            .expect("run isolated UI fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            should_pass,
            "{fixture} fixture had unexpected status:\n{stderr}"
        );
        if !should_pass {
            let expected =
                std::fs::read_to_string(root.join(format!("tests/fixtures/{fixture}/stderr.txt")))
                    .unwrap();
            for signature in expected.lines() {
                assert!(
                    stderr.contains(signature),
                    "{fixture} missing diagnostic `{signature}`:\n{stderr}"
                );
            }
            assert!(
                !stderr.contains("Could not find `zerocopy`"),
                "dependency-polluted diagnostic:\n{stderr}"
            );
        }
    }

    let encode_manifest = root.join("tests/fixtures/encode-api-pass/Cargo.toml");
    let encode = Command::new("cargo")
        .args(["+1.85.0", "run", "--locked", "--manifest-path"])
        .arg(&encode_manifest)
        .args(["--bin", "encode-api-pass"])
        .arg("--target-dir")
        .arg(root.join("../target/ui-fixtures/encode-api-pass"))
        .output()
        .expect("run encode API fixture");
    assert!(
        encode.status.success(),
        "encode API fixture failed:\n{}",
        String::from_utf8_lossy(&encode.stderr)
    );
    let absent = Command::new("cargo")
        .args(["+1.85.0", "check", "--locked", "--manifest-path"])
        .arg(&encode_manifest)
        .args(["--bin", "generic-encode-absent", "--target-dir"])
        .arg(root.join("../target/ui-fixtures/generic-encode-absent"))
        .output()
        .expect("check generic encode absence fixture");
    let absent_stderr = String::from_utf8_lossy(&absent.stderr);
    assert!(
        !absent.status.success(),
        "generic schemas unexpectedly exposed encode()"
    );
    assert!(
        absent_stderr.matches("no method named `encode`").count() >= 2,
        "missing generic encode diagnostics:\n{absent_stderr}"
    );

    let missing_manifest = root.join("tests/fixtures/missing-zerocopy-fail/Cargo.toml");
    let missing = Command::new("cargo")
        .args(["+1.85.0", "check", "--locked", "--manifest-path"])
        .arg(&missing_manifest)
        .arg("--target-dir")
        .arg(root.join("../target/ui-fixtures/missing-zerocopy-fail"))
        .output()
        .expect("check missing-zerocopy fixture");
    let missing_stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        !missing.status.success(),
        "missing-zerocopy struct unexpectedly compiled"
    );
    let expected =
        std::fs::read_to_string(root.join("tests/fixtures/missing-zerocopy-fail/stderr.txt"))
            .unwrap();
    for signature in expected.lines() {
        assert!(
            missing_stderr.contains(signature),
            "missing targeted dependency diagnostic `{signature}`:\n{missing_stderr}"
        );
    }

    let pass_manifest = root.join("tests/fixtures/lazy-zero-pass/Cargo.toml");
    let pass = Command::new("cargo")
        .args(["+1.85.0", "build", "--locked", "--manifest-path"])
        .arg(&pass_manifest)
        .arg("--target-dir")
        .arg(root.join("../target/ui-fixtures/lazy-zero-pass"))
        .output()
        .expect("build lazy-zero pass fixture");
    assert!(
        pass.status.success(),
        "lazy-zero nonzero neighbors failed:\n{}",
        String::from_utf8_lossy(&pass.stderr)
    );

    let fail_manifest = root.join("tests/fixtures/lazy-zero-fail/Cargo.toml");
    let expected =
        std::fs::read_to_string(root.join("tests/fixtures/lazy-zero-fail/stderr.txt")).unwrap();
    for bin in ["root-parse", "root-encode", "parent-parse", "parent-encode"] {
        let output = Command::new("cargo")
            .args(["+1.85.0", "build", "--locked", "--manifest-path"])
            .arg(&fail_manifest)
            .args(["--bin", bin, "--target-dir"])
            .arg(root.join(format!("../target/ui-fixtures/lazy-zero-fail-{bin}")))
            .output()
            .expect("build lazy-zero failure fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "lazy-zero {bin} unexpectedly compiled"
        );
        for signature in expected.lines() {
            assert!(
                stderr.contains(signature),
                "lazy-zero {bin} missing `{signature}`:\n{stderr}"
            );
        }
        assert!(
            !stderr.contains("Could not find `zerocopy`"),
            "dependency-polluted diagnostic:\n{stderr}"
        );
    }
}
