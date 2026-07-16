#[path = "build/codec_cpp.rs"]
mod codec_cpp;
#[path = "build/frontend.rs"]
mod frontend;
#[path = "build/layout_cpp.rs"]
mod layout_cpp;

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn fail(message: impl std::fmt::Display) -> ! {
    panic!("zero-schema conformance: {message}")
}

fn parse(path: &Path) -> syn::File {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| fail(format!("read {}: {error}", path.display())));
    syn::parse_file(&source)
        .unwrap_or_else(|error| fail(format!("parse {}: {error}", path.display())))
}

fn profile(target: &str) -> &'static str {
    match target {
        "aarch64-apple-darwin" => "macos-aarch64-le",
        "x86_64-unknown-linux-gnu" => "linux-x86_64-le",
        "i686-unknown-linux-gnu" => "linux-i686-le",
        "x86_64-pc-windows-msvc" => "windows-x86_64-msvc-le",
        "powerpc64-unknown-linux-gnu" => "linux-powerpc64-be",
        _ => fail(format!("unsupported target {target}")),
    }
}

fn contract(model: &frontend::Model, profile: &str, target: &str) -> String {
    let mut output = format!(
        "#[allow(dead_code)]\npub(crate) const BUILD_PROFILE: &str = {profile:?};\n#[allow(dead_code)]\npub(crate) const BUILD_TARGET: &str = {target:?};\npub(crate) const BUILD_CASES: &[BuildCaseContract] = &[\n"
    );
    for case in &model.cases {
        let layout: Vec<_> = case.layout.iter().map(|entry| entry.key).collect();
        let observations: Vec<_> = case.observe.iter().map(|entry| entry.key).collect();
        writeln!(
            output,
            "BuildCaseContract{{case_id:{},root_id:{:?},layout_keys:&{:?},observation_keys:&{:?}}},",
            case.id, case.root_id, layout, observations
        ).unwrap();
    }
    output.push_str("];\n");
    output
}

fn cpp(model: &frontend::Model, target_endian: &str, pointer_width: &str) -> String {
    let layout = layout_cpp::emit(model);
    let mut output = String::from(
        "#include <cstddef>\n#include <cstdint>\n#include <cstring>\n#include <climits>\n#include <limits>\n#include <type_traits>\n",
    );
    writeln!(
        output,
        "static_assert(CHAR_BIT == 8); static_assert(sizeof(void*) * CHAR_BIT == {pointer_width});"
    )
    .unwrap();
    output.push_str("static_assert(sizeof(std::uint8_t)==1 && sizeof(std::uint16_t)==2 && sizeof(std::uint32_t)==4 && sizeof(std::uint64_t)==8);\n");
    output.push_str("static_assert(std::numeric_limits<float>::is_iec559 && sizeof(float)==4); static_assert(std::numeric_limits<double>::is_iec559 && sizeof(double)==8);\n");
    if target_endian == "little" {
        output.push_str("#if defined(__BYTE_ORDER__)\nstatic_assert(__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__);\n#endif\n");
    } else {
        output.push_str("#if defined(__BYTE_ORDER__)\nstatic_assert(__BYTE_ORDER__ == __ORDER_BIG_ENDIAN__);\n#endif\n");
    }
    output.push_str(&layout.declarations);
    output.push_str(&layout.assertions);
    codec_cpp::emit(model, &layout, &mut output);
    output
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let corpus_path = manifest.join("../test-fixtures/schema-corpus/src/conformance.rs");
    let cases_path = manifest.join("fixtures/cases.rs");
    println!("cargo:rerun-if-changed={}", corpus_path.display());
    println!("cargo:rerun-if-changed={}", cases_path.display());
    for variable in [
        "CXX",
        "CXXFLAGS",
        "CRATE_CC_NO_DEFAULTS",
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ENV",
        "CARGO_CFG_TARGET_ENDIAN",
        "CARGO_CFG_TARGET_POINTER_WIDTH",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    let model = frontend::parse(&parse(&corpus_path), &parse(&cases_path));
    let target = env::var("TARGET").unwrap();
    let endian = env::var("CARGO_CFG_TARGET_ENDIAN").unwrap();
    let pointer_width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap();
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        output_dir.join("case_contract.rs"),
        contract(&model, profile(&target), &target),
    )
    .unwrap();
    let cpp_path = output_dir.join("zero_schema_conformance.cc");
    fs::write(&cpp_path, cpp(&model, &endian, &pointer_width)).unwrap();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .warnings(true)
        .warnings_into_errors(true)
        .file(cpp_path);
    let is_msvc = build.get_compiler().is_like_msvc();
    if is_msvc {
        // C4324 reports the deliberate padding introduced by generated `alignas` layouts.
        build.flag("/permissive-").flag("/W4").flag("/wd4324");
    } else {
        for flag in [
            "-Wall",
            "-Wextra",
            "-Wpedantic",
            "-Wconversion",
            "-Wsign-conversion",
        ] {
            build.flag_if_supported(flag);
        }
    }
    let compiler = build.get_compiler();
    let mut version = std::process::Command::new(compiler.path());
    version.args(compiler.args()).arg("--version");
    for (key, value) in compiler.env() {
        version.env(key, value);
    }
    let mut version_output = version.output().unwrap_or_else(|error| {
        fail(format!(
            "execute selected C++ compiler {}: {error}",
            compiler.path().display()
        ))
    });
    if !version_output.status.success() && compiler.is_like_msvc() {
        let mut msvc_version = std::process::Command::new(compiler.path());
        msvc_version.args(compiler.args()).arg("/?");
        for (key, value) in compiler.env() {
            msvc_version.env(key, value);
        }
        version_output = msvc_version.output().unwrap_or_else(|error| {
            fail(format!(
                "execute selected MSVC compiler {}: {error}",
                compiler.path().display()
            ))
        });
    }
    let evidence = format!(
        "path={}\nargs={:?}\nstatus={}\nstdout:\n{}\nstderr:\n{}",
        compiler.path().display(),
        compiler.args(),
        version_output.status,
        String::from_utf8_lossy(&version_output.stdout),
        String::from_utf8_lossy(&version_output.stderr),
    );
    fs::write(output_dir.join("selected-cxx.txt"), evidence).unwrap();
    build.compile("zero_schema_conformance_cpp");
}
