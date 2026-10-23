fn main() {
    let manifest = format!("{}/app.manifest", env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed={manifest}");
    // asInvoker, per-monitor-v2 DPI and long paths, embedded without an extra crate.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{manifest}");
}
