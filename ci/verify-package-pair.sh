#!/bin/sh
set -eu
[ "$#" -eq 2 ] || { echo "usage: $0 DERIVE.crate RUNTIME.crate" >&2; exit 64; }
derive=$1 runtime=$2; [ -f "$derive" ] && [ -f "$runtime" ] || exit 64
tmp=$(mktemp -d)
cleanup() { rm -rf "$tmp"; }
terminate() { signal=$1; trap - "$signal"; kill -s "$signal" "$$"; }
trap cleanup EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM
tar -xzf "$derive" -C "$tmp"; tar -xzf "$runtime" -C "$tmp"
droot=$(printf '%s\n' "$tmp"/zero-schema-derive-*); rroot=$(printf '%s\n' "$tmp"/zero-schema-[0-9]*)
for root in "$droot" "$rroot"; do [ -f "$root/Cargo.toml" ] && [ -f "$root/Cargo.toml.orig" ] || { echo 'archive lacks generated/original manifest pair' >&2; exit 1; }; done
cmp "$droot/LICENSE" "$rroot/LICENSE"
check_orig() { file=$1; shift; for exact do grep -Fqx "$exact" "$file" || { echo "$file: missing exact metadata: $exact" >&2; exit 1; }; done; }
check_orig "$droot/Cargo.toml.orig" 'name = "zero-schema-derive"' 'version = "0.1.0"' 'edition = "2024"' 'rust-version = "1.85"' 'license = "MIT"' 'description = "Derive macro for zero-schema"' 'documentation = "https://docs.rs/zero-schema-derive"' 'include = ["src/**", "Cargo.toml", "LICENSE"]'
check_orig "$rroot/Cargo.toml.orig" 'name = "zero-schema"' 'version = "0.1.0"' 'edition = "2024"' 'rust-version = "1.85"' 'license = "MIT"' 'description = "Fixed-layout, zero-copy Rust schemas for shared memory and C++ interoperability"' 'documentation = "https://docs.rs/zero-schema"' 'readme = "README.md"' 'include = ["src/**", "examples/**", "benches/**", "Cargo.toml", "README.md", "TESTING.md", "LICENSE", "SAFETY.md", "CHANGELOG.md"]'
verify_metadata() {
    manifest=$1 name=$2 description=$3 documentation=$4 readme=$5
    cargo +1.85.0 metadata --no-deps --format-version 1 --manifest-path "$manifest" >"$tmp/meta.json"
    META=$tmp/meta.json EXPECT_NAME=$name EXPECT_DESC=$description EXPECT_DOC=$documentation EXPECT_README=$readme ruby -rjson -e 'p=JSON.parse(File.read(ENV.fetch("META"))).fetch("packages").first; expected={"name"=>ENV.fetch("EXPECT_NAME"),"version"=>"0.1.0","edition"=>"2024","rust_version"=>"1.85","license"=>"MIT","description"=>ENV.fetch("EXPECT_DESC"),"documentation"=>ENV.fetch("EXPECT_DOC")}; expected.each{|k,v| abort("generated manifest #{k} mismatch") unless p[k]==v}; r=ENV.fetch("EXPECT_README"); abort("generated manifest readme mismatch") unless (r.empty? ? p["readme"].nil? : File.basename(p["readme"])==r)'
}
verify_metadata "$droot/Cargo.toml" zero-schema-derive 'Derive macro for zero-schema' 'https://docs.rs/zero-schema-derive' ''
verify_metadata "$rroot/Cargo.toml" zero-schema 'Fixed-layout, zero-copy Rust schemas for shared memory and C++ interoperability' 'https://docs.rs/zero-schema' README.md
cargo +1.85.0 check --manifest-path "$droot/Cargo.toml" --locked

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
zero-schema-derive={path="$droot"}
EOF
plain='#![no_std]
pub fn size() -> usize { core::mem::size_of::<zs::LayoutError>() }'
derive_source='#![no_std]
use zs::ZeroSchema;
#[derive(Debug, PartialEq, ZeroSchema)]
pub struct Probe { pub sequence: u32, pub ready: bool }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregate_round_trip() {
        let value = Probe { sequence: 0x0102_0304, ready: true };
        let buffer = value.encode().unwrap();
        let decoded = Probe::parse(buffer.as_bytes()).unwrap();
        assert_eq!(decoded, value);
    }
}'
printf '%s\n' "$plain" >"$graph/src/lib.rs"
cargo +1.85.0 generate-lockfile --manifest-path "$graph/Cargo.toml"
for features in '' alloc std; do if [ -n "$features" ]; then cargo +1.85.0 check --locked --manifest-path "$graph/Cargo.toml" --features "zs/$features"; else cargo +1.85.0 check --locked --manifest-path "$graph/Cargo.toml"; fi; done
printf '%s\n' "$derive_source" >"$graph/src/lib.rs"
cargo +1.85.0 check --locked --manifest-path "$graph/Cargo.toml" --features 'zs/derive'
cargo +1.85.0 check --locked --manifest-path "$graph/Cargo.toml" --features 'zs/alloc,zs/derive'
cargo +1.85.0 test --locked --manifest-path "$graph/Cargo.toml" --features 'zs/derive'
RUSTDOCFLAGS='-D warnings' cargo +1.85.0 doc --locked --manifest-path "$graph/Cargo.toml" --features 'zs/derive' --no-deps
mkdir -p "$rroot/.cargo"
cat >"$rroot/.cargo/config.toml" <<EOF
[patch.crates-io]
zero-schema-derive={path="$droot"}
EOF
(cd "$rroot" && cargo +1.85.0 generate-lockfile --manifest-path Cargo.toml)
(cd "$rroot" && cargo +1.85.0 check --locked --manifest-path Cargo.toml --benches --all-features)
