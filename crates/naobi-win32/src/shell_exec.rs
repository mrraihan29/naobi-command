//! Opens a path, a `shell:AppsFolder\<AUMID>` item or a URL with the shell (the Launch effect).
use std::path::Path;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// ShellExecuteW reports success with a value above this.
const SUCCESS_ABOVE: isize = 32;

/// Runs the shell's "open" verb on `target`. The error is the shell's own code; the target is not
/// part of it, so a caller can log it without logging a path.
pub fn open(target: &str) -> std::io::Result<()> {
    open_with(target, None, None)
}

/// ShellExecuteW "open" on `file` with `params` and working folder `dir`, SW_SHOWNORMAL. Verb
/// "open" only, never "runas" (T-024). The error is the shell's own code, with no text from the
/// arguments in it.
pub fn open_with(file: &str, params: Option<&str>, dir: Option<&Path>) -> std::io::Result<()> {
    shell_execute(w!("open"), file, params, dir)
}

/// Launches `target` elevated using the shell `"runas"` verb (explicit user Action Panel choice).
pub fn run_as_admin(target: &str) -> std::io::Result<()> {
    shell_execute(w!("runas"), target, None, None)
}

/// Opens Explorer and selects `target`, falling back to opening its parent folder if selection fails.
pub fn reveal_in_explorer(target: &str) -> std::io::Result<()> {
    let wtarget = HSTRING::from(target);
    // SAFETY: COM is initialized for this thread and cleaned up before return; `pidl` is freed
    // whenever `ILCreateFromPathW` returns non-null.
    let revealed = unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let pidl = ILCreateFromPathW(&wtarget);
        let ok = if !pidl.is_null() {
            let res = SHOpenFolderAndSelectItems(pidl, None, 0).is_ok();
            ILFree(Some(pidl));
            res
        } else {
            false
        };
        if com {
            CoUninitialize();
        }
        ok
    };
    if revealed {
        return Ok(());
    }
    let fallback = Path::new(target)
        .parent()
        .and_then(|p| p.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(target);
    open(fallback)
}

fn shell_execute(
    verb: PCWSTR,
    file: &str,
    params: Option<&str>,
    dir: Option<&Path>,
) -> std::io::Result<()> {
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from).unwrap_or_default();
    let dir = dir
        .map(|d| HSTRING::from(d.as_os_str()))
        .unwrap_or_default();
    // SAFETY: COM is initialised and released on this thread around the call; `file`, `params`,
    // `dir` (an empty HSTRING is a null pointer) and `verb` outlive the call.
    let code = unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let r = ShellExecuteW(None, verb, &file, &params, &dir, SW_SHOWNORMAL);
        if com {
            CoUninitialize();
        }
        r.0 as isize
    };
    if code > SUCCESS_ABOVE {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(code as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(open(r"C:\this\path\does\not\exist-naobi.exe").is_err());
    }
}
