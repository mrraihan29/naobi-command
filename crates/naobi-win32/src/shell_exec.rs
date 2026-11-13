//! Opens a path, a `shell:AppsFolder\<AUMID>` item or a URL with the shell (the Launch effect).
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// ShellExecuteW reports success with a value above this.
const SUCCESS_ABOVE: isize = 32;

/// Runs the shell's "open" verb on `target`. The error is the shell's own code; the target is not
/// part of it, so a caller can log it without logging a path.
pub fn open(target: &str) -> std::io::Result<()> {
    let file = HSTRING::from(target);
    // SAFETY: COM is initialised and released on this thread around the call; `file` and the static
    // verb outlive it and the other pointers are null.
    let code = unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let r = ShellExecuteW(
            None,
            w!("open"),
            &file,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
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
