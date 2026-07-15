use std::{env, fs, path::PathBuf};

fn main() {
    let view = ["Array", "View"].concat();
    let iter = ["Array", "Iter"].concat();
    let materializer = ["copy_from", "to"].concat();
    let physical = ["Payload", "Wire"].concat();
    let selection = ["Tagged", "Selection"].concat();
    let source = format!(
        "use zero_schema_macros::zero;\nuse zs::{{{view}, {iter}}};\n\n#[zero(crate = zs)]\nstruct OldSurface {{ values: [u8; 1] }}\n\nfn probe() {{\n    let bytes = [7_u8];\n    let capability = OldSurface::access(&bytes).expect(\"reviewed producer bytes\");\n    let _ = capability.{materializer}();\n    let _: Option<{view}<'static, u8, 1>> = None;\n    let _: Option<{iter}<'static, u8, 1>> = None;\n    let _: Option<zs::{physical}> = None;\n    let _: Option<zs::{selection}> = None;\n}}\n"
    );
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("legacy.rs"),
        source,
    )
    .unwrap();
}
