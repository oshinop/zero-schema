#!/bin/sh
set -eu
[ "$#" -eq 2 ] || { echo "usage: $0 MACROS.crate RUNTIME.crate" >&2; exit 64; }
macros=$1 runtime=$2; [ -f "$macros" ] && [ -f "$runtime" ] || exit 64
tmp=$(mktemp -d)
cleanup() { rm -rf "$tmp"; }
terminate() { signal=$1; trap - "$signal"; kill -s "$signal" "$$"; }
trap cleanup EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM
tar -xzf "$macros" -C "$tmp"; tar -xzf "$runtime" -C "$tmp"
mroot=$(printf '%s\n' "$tmp"/zero-schema-macros-*); rroot=$(printf '%s\n' "$tmp"/zero-schema-[0-9]*)
for root in "$mroot" "$rroot"; do [ -f "$root/Cargo.toml" ] && [ -f "$root/Cargo.toml.orig" ] || { echo 'archive lacks generated/original manifest pair' >&2; exit 1; }; done
cmp "$mroot/LICENSE" "$rroot/LICENSE"
check_orig() { file=$1; shift; for exact do grep -Fqx "$exact" "$file" || { echo "$file: missing exact metadata: $exact" >&2; exit 1; }; done; }
check_orig "$mroot/Cargo.toml.orig" 'name = "zero-schema-macros"' 'version = "0.1.0"' 'edition = "2024"' 'rust-version = "1.85"' 'license = "MIT"' 'description = "Attribute macros for zero-schema"' 'documentation = "https://docs.rs/zero-schema-macros"' 'include = ["src/**", "Cargo.toml", "LICENSE"]'
check_orig "$rroot/Cargo.toml.orig" 'name = "zero-schema"' 'version = "0.1.0"' 'edition = "2024"' 'rust-version = "1.85"' 'license = "MIT"' 'description = "Fixed-layout, zero-copy Rust schemas for shared memory and C++ interoperability"' 'documentation = "https://docs.rs/zero-schema"' 'readme = "README.md"' 'include = ["src/**", "examples/**", "benches/**", "Cargo.toml", "README.md", "TESTING.md", "LICENSE", "SAFETY.md", "CHANGELOG.md"]'
verify_metadata() {
    manifest=$1 name=$2 description=$3 documentation=$4 readme=$5
    cargo +1.85.0 metadata --offline --no-deps --format-version 1 --manifest-path "$manifest" >"$tmp/meta.json"
    META=$tmp/meta.json EXPECT_NAME=$name EXPECT_DESC=$description EXPECT_DOC=$documentation EXPECT_README=$readme ruby -rjson -e 'p=JSON.parse(File.read(ENV.fetch("META"))).fetch("packages").first; expected={"name"=>ENV.fetch("EXPECT_NAME"),"version"=>"0.1.0","edition"=>"2024","rust_version"=>"1.85","license"=>"MIT","description"=>ENV.fetch("EXPECT_DESC"),"documentation"=>ENV.fetch("EXPECT_DOC")}; expected.each{|k,v| abort("generated manifest #{k} mismatch") unless p[k]==v}; r=ENV.fetch("EXPECT_README"); abort("generated manifest readme mismatch") unless (r.empty? ? p["readme"].nil? : File.basename(p["readme"])==r)'
}
verify_metadata "$mroot/Cargo.toml" zero-schema-macros 'Attribute macros for zero-schema' 'https://docs.rs/zero-schema-macros' ''
verify_metadata "$rroot/Cargo.toml" zero-schema 'Fixed-layout, zero-copy Rust schemas for shared memory and C++ interoperability' 'https://docs.rs/zero-schema' README.md
cargo +1.85.0 check --offline --locked --manifest-path "$mroot/Cargo.toml"

graph=$tmp/graph; mkdir -p "$graph/src"
cat >"$graph/Cargo.toml" <<EOF
[package]
name="package-pair-consumer"
version="0.0.0"
edition="2024"
publish=false
[workspace]
[dependencies]
zs={package="zero-schema",path="$rroot",default-features=false}
zerocopy={version="=0.8.54",default-features=false,features=["derive"]}
[patch.crates-io]
zero-schema-macros={path="$mroot"}
EOF
plain='#![no_std]
pub fn size() -> usize { core::mem::size_of::<zs::LayoutError>() }'
macro_source='#![no_std]
use zs::zero;

#[zero]
#[derive(Debug, PartialEq)]
pub struct Probe {
    pub sequence: u32,
    pub ready: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_access_materialization_mutation_and_receiving_storage() {
        let mut storage = zs::schema_buffer!(Probe);
        assert_eq!(storage.as_bytes().len(), Probe::SCHEMA_SIZE);
        assert_eq!(storage.as_bytes().as_ptr() as usize % Probe::SCHEMA_ALIGN, 0);

        let bytes = storage.as_bytes_mut();
        bytes[..4].copy_from_slice(&0x0102_0304_u32.to_ne_bytes());
        bytes[4] = 1;

        let view = Probe::access(storage.as_bytes()).expect("producer initialized valid bytes");
        assert_eq!(view.sequence(), 0x0102_0304);
        assert!(view.ready());
        assert_eq!(view.copy_into(), Probe { sequence: 0x0102_0304, ready: true });

        let mut view = Probe::access_mut(storage.as_bytes_mut()).expect("valid producer bytes remain mutable");
        view.sequence_mut().set(43).expect("field mutation");
        view.ready_mut().set(false).expect("boolean mutation");
        assert_eq!(view.copy_into(), Probe { sequence: 43, ready: false });
    }
}'
printf '%s\n' "$plain" >"$graph/src/lib.rs"
cargo +1.85.0 generate-lockfile --offline --manifest-path "$graph/Cargo.toml"
cargo +1.85.0 update --offline --manifest-path "$graph/Cargo.toml" -p syn --precise 2.0.118
printf '%s\n' "$macro_source" >"$graph/src/lib.rs"
cargo +1.85.0 check --offline --manifest-path "$graph/Cargo.toml"
printf '%s\n' "$plain" >"$graph/src/lib.rs"
for features in '' alloc std; do if [ -n "$features" ]; then cargo +1.85.0 check --offline --locked --manifest-path "$graph/Cargo.toml" --features "zs/$features"; else cargo +1.85.0 check --offline --locked --manifest-path "$graph/Cargo.toml"; fi; done
printf '%s\n' "$macro_source" >"$graph/src/lib.rs"
cargo +1.85.0 check --offline --locked --manifest-path "$graph/Cargo.toml"
cargo +1.85.0 check --offline --locked --manifest-path "$graph/Cargo.toml" --features 'zs/alloc'
cargo +1.85.0 test --offline --locked --manifest-path "$graph/Cargo.toml"
RUSTDOCFLAGS='-D warnings' cargo +1.85.0 doc --offline --locked --manifest-path "$graph/Cargo.toml" --no-deps
mkdir -p "$rroot/.cargo"
cat >"$rroot/.cargo/config.toml" <<EOF
[patch.crates-io]
zero-schema-macros={path="$mroot"}
EOF
(cd "$rroot" && cargo +1.85.0 generate-lockfile --offline --manifest-path Cargo.toml)
(cd "$rroot" && cargo +1.85.0 check --offline --locked --manifest-path Cargo.toml --benches --all-features)
