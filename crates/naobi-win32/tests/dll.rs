//! T-025: a DLL planted next to the exe is not loaded; only System32 and WinSxS are searched.
use std::path::Path;

use naobi_win32::dll_hardening::harden;
use windows::core::HSTRING;
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::LoadLibraryW;

/// The first statement of `main` is the hardening call (T-025: "the first call in main").
#[test]
fn d025_harden_is_the_first_call_in_main() {
    let main_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../naobi-app/src/main.rs");
    let text = std::fs::read_to_string(&main_rs).expect("read naobi-app main.rs");
    let body = text
        .split("fn main() -> anyhow::Result<()> {")
        .nth(1)
        .expect("main has a body");
    let first = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .expect("main has a statement");
    assert_eq!(
        first,
        "naobi_win32::dll_hardening::harden(); // T-025: first call"
    );
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d025_loaded_modules_from_trusted_dirs() {
    // Plant a real DLL under a unique name in the folder the loader searched by default.
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let source = Path::new(&system_root).join("System32").join("version.dll");
    let exe_dir = std::env::current_exe()
        .expect("test exe path")
        .parent()
        .expect("exe folder")
        .to_path_buf();
    let planted = exe_dir.join("naobi_d025_planted.dll");
    std::fs::copy(&source, &planted).expect("plant the DLL next to the exe");

    harden();
    let name = HSTRING::from("naobi_d025_planted.dll");
    // SAFETY: loads a DLL by name; a successful load is freed again below.
    let loaded = unsafe { LoadLibraryW(&name) };
    if let Ok(module) = loaded {
        // SAFETY: frees the module this call just loaded.
        let _ = unsafe { FreeLibrary(module) };
    }
    let _ = std::fs::remove_file(&planted);
    assert!(
        loaded.is_err(),
        "the planted DLL next to the exe was loaded"
    );
}
