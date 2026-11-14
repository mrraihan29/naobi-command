//! Opens a path, a `shell:AppsFolder\<AUMID>` item or a URL with the shell (the Launch effect).
use windows::core::{w, HSTRING};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::ShellExecuteW;
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
pub fn open_with(
    file: &str,
    params: Option<&str>,
    dir: Option<&std::path::Path>,
) -> std::io::Result<()> {
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from).unwrap_or_default();
    let dir = dir
        .map(|d| HSTRING::from(d.as_os_str()))
        .unwrap_or_default();
    // SAFETY: COM is initialised and released on this thread around the call; `file`, `params` and
    // `dir` (an empty HSTRING is a null pointer) and the static verb outlive it.
    let code = unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let r = ShellExecuteW(None, w!("open"), &file, &params, &dir, SW_SHOWNORMAL);
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
