//! R-005: the MVP build loads no external plugin code and makes no AI calls. Every module is a Rust crate of
//! this workspace registered in `modules/mod.rs`; nothing loads a library at run time; no HTTP, AI or loader
//! crate is in the build; and the dependency bans that keep it so are in `deny.toml`.
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The `src` files of every crate, without tests (which may load a planted DLL on purpose, T-025).
fn product_sources() -> Vec<(String, String)> {
    let mut crates = Vec::new();
    rust_files(&root().join("crates"), &mut crates);
    crates
        .into_iter()
        .filter(|p| p.components().any(|c| c.as_os_str() == "src"))
        .filter_map(|p| {
            let name = p.strip_prefix(root()).unwrap_or(&p).display().to_string();
            Some((name.replace('\\', "/"), std::fs::read_to_string(&p).ok()?))
        })
        .collect()
}

// @R-005
#[test]
fn r005_no_dynamic_loading_no_ai_calls() {
    let sources = product_sources();
    assert!(
        sources.len() > 50,
        "the scan found the product: {}",
        sources.len()
    );

    // No run-time library loading anywhere in the product. SetDefaultDllDirectories (the hardening call,
    // T-025) restricts loading and is the one place the loader is touched.
    let loaders = [
        "LoadLibrary",
        "LoadPackagedLibrary",
        "GetProcAddress",
        "libloading",
        "dlopen",
    ];
    for (file, text) in &sources {
        for word in loaders {
            assert!(!text.contains(word), "{file} uses {word}");
        }
    }
    assert!(
        sources
            .iter()
            .any(|(f, t)| f.ends_with("dll_hardening.rs") && t.contains("SetDefaultDllDirectories")),
        "the DLL hardening call is where it is expected"
    );

    // No AI service named anywhere in the product's source or manifests.
    let ai = ["openai", "anthropic", "ollama", "gemini", "chatgpt"];
    for (file, text) in &sources {
        let lower = text.to_ascii_lowercase();
        for word in ai {
            assert!(!lower.contains(word), "{file} names an AI service: {word}");
        }
    }

    // Modules are registered in one place, and only built-in sources exist: a source is registered from
    // `crates/naobi-app/src/modules`, never from anywhere else.
    for (file, text) in &sources {
        let registers = text.contains(".register_source(") || text.contains(".register_view(");
        let allowed = file.starts_with("crates/naobi-app/src/modules/")
            || file.starts_with("crates/naobi-app/src/controller.rs")
            || file.starts_with("crates/naobi-app/src/lib.rs")
            || file.starts_with("crates/naobi-app/src/perf_fixture.rs");
        assert!(
            !registers || allowed,
            "{file} registers a source outside the module registry"
        );
    }

    // The bans that keep HTTP, AI and loader crates out are in the policy file.
    // ADR-0008 permits Win32_Networking_WinHttp solely inside naobi-win32.
    let deny = std::fs::read_to_string(root().join("deny.toml"))
        .unwrap_or_else(|e| panic!("deny.toml: {e}"));
    for banned in [
        "reqwest",
        "hyper",
        "ureq",
        "isahc",
        "curl",
        "surf",
        "attohttpc",
        "libloading",
        "dlopen2",
        "Win32_Networking_WinInet",
        "Win32_Networking_WinSock",
    ] {
        assert!(deny.contains(banned), "deny.toml does not ban {banned}");
    }
    assert!(
        !deny.contains("\"Win32_Networking_WinHttp\""),
        "deny.toml must allow Win32_Networking_WinHttp per ADR-0008"
    );
    let win32_manifest = std::fs::read_to_string(root().join("crates/naobi-win32/Cargo.toml"))
        .unwrap_or_else(|e| panic!("naobi-win32/Cargo.toml: {e}"));
    assert!(
        win32_manifest.contains("Win32_Networking_WinHttp"),
        "crates/naobi-win32/Cargo.toml must enable Win32_Networking_WinHttp"
    );
    for entry in std::fs::read_dir(root().join("crates"))
        .unwrap_or_else(|e| panic!("crates/: {e}"))
        .flatten()
    {
        let p = entry.path();
        if p.file_name().is_some_and(|n| n == "naobi-win32") {
            continue;
        }
        let manifest = p.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            assert!(
                !text.contains("Win32_Networking_WinHttp"),
                "{} must not enable Win32_Networking_WinHttp",
                manifest.display()
            );
        }
    }

    // And the lockfile holds none of the network or AI client crates. `libloading` is in the lockfile
    // for platforms the build never targets; `deny.toml` checks the Windows graph only.
    let lock = std::fs::read_to_string(root().join("Cargo.lock"))
        .unwrap_or_else(|e| panic!("Cargo.lock: {e}"));
    for krate in [
        "reqwest",
        "hyper",
        "ureq",
        "isahc",
        "curl",
        "surf",
        "attohttpc",
        "dlopen2",
        "async-openai",
        "ollama-rs",
        "rig-core",
        "genai",
    ] {
        assert!(
            !lock.contains(&format!("name = \"{krate}\"")),
            "Cargo.lock holds {krate}"
        );
    }
}
