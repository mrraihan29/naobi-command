const TARGET: &str = "x86_64-pc-windows-msvc";

// @R-003
#[test]
fn r003_release_target_is_x86_64_msvc() {
    const {
        assert!(cfg!(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc"
        )))
    };
    let read = |rel: &str| {
        std::fs::read_to_string(format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    };
    // No other target is configured anywhere.
    for line in read(".cargo/config.toml").lines() {
        let l = line.trim();
        if l.starts_with("[target.") {
            assert_eq!(l, format!("[target.{TARGET}]"));
        }
        assert!(!l.starts_with("target ="), "build.target must stay unset");
    }
    let want = format!("targets = [\"{TARGET}\"]");
    assert!(read("rust-toolchain.toml").contains(&want));
    assert!(read("deny.toml").contains(&want));
}

// Guard for the opt-in lint floor: a crate that forgets it could use unsafe and unwrap silently.
#[test]
fn every_crate_opts_into_workspace_lints() {
    let crates = format!("{}/..", env!("CARGO_MANIFEST_DIR"));
    for entry in std::fs::read_dir(crates).unwrap() {
        let dir = entry.unwrap().path();
        if dir.ends_with("naobi-win32") {
            continue; // EX-001: carries its own [lints.*] tables
        }
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(
            manifest.contains("[lints]\nworkspace = true")
                || manifest.contains("[lints]\r\nworkspace = true"),
            "{} must set [lints] workspace = true",
            dir.display()
        );
    }
}
