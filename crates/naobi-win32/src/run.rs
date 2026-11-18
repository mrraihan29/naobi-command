//! The `>` run command (R-024): finds a program, tells GUI from console, runs a line in a new
//! PowerShell console that stays open, starts a GUI exe. Nothing here logs the line, a token, a path
//! or arguments (D-020). The console is never put in the kill-on-close job, so it outlives the app.
use std::ffi::{c_void, OsStr};
use std::io::Read;
use std::path::{Path, PathBuf};

use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Storage::FileSystem::GetDriveTypeW;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

use crate::shell_exec::open_with;

/// A program the first token of a line resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    /// True only for a readable PE image with the WINDOWS_GUI subsystem.
    pub gui: bool,
}

/// `GetDriveTypeW` for a network drive.
const DRIVE_REMOTE: u32 = 4;
const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\";
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
/// The PE header of every real exe sits well inside this.
// ponytail: a header past 4096 bytes reads as "not GUI"; read at e_lfanew if one ever needs it.
const HEAD_BYTES: u64 = 4096;
/// CreateProcess limit (32767 chars) with room for the exe path and `-NoExit -EncodedCommand`.
const MAX_PARAMS: usize = 30_000;

/// Resolves the first token of a `>` line, as PowerShell would find an external program: a token
/// with `\`, `/` or `:` is a path (as is, then with each PATHEXT extension when it has none); a bare
/// name is tried in each PATH directory in order, then in App Paths (HKCU, then HKLM). UNC and
/// remote-drive candidates are skipped without touching them (T-035).
pub fn find_program(token: &str) -> Option<Found> {
    if token.is_empty() || token.contains(['"', '\0']) {
        return None;
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| DEFAULT_PATHEXT.to_string());
    if let Some(f) = candidates(token, &path_var, &pathext)
        .iter()
        .find_map(|c| probe(c))
    {
        return Some(f);
    }
    if is_path_token(token) {
        return None;
    }
    let name = if token.to_ascii_lowercase().ends_with(".exe") {
        token.to_string()
    } else {
        format!("{token}.exe")
    };
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| app_path(root, &name))
        .find_map(|p| probe(Path::new(&p)))
}

/// Runs `line` in a new console window that stays open (an Alacritty window when it is installed under
/// %LOCALAPPDATA%\Alacritty, else the default console host):
/// `<shell> -NoExit -EncodedCommand <base64>`, the profile loads, working folder %USERPROFILE%.
/// `ErrorKind::NotFound` when neither pwsh.exe nor powershell.exe resolves.
pub fn run_in_console(line: &str) -> std::io::Result<()> {
    let shell = pick_shell(find_program)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let params = format!(
        "-NoExit -EncodedCommand {}",
        encoded_command(&callable(line))
    );
    // The Owner's terminal, when it is installed: the same shell and line, in an Alacritty window.
    let (file, params) = match alacritty() {
        Some(term) => (term, format!("-e \"{shell}\" {params}")),
        None => (shell, params),
    };
    if params.len() > MAX_PARAMS {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    open_with(&file, Some(&params), home().as_deref())
}

/// `%LOCALAPPDATA%/Alacritty/alacritty.exe` when it exists on a local path (never a UNC one, T-035).
fn alacritty() -> Option<String> {
    let path = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("Alacritty")
        .join("alacritty.exe");
    let text = path.to_str()?;
    (!text.starts_with(r"\\") && path.is_file()).then(|| text.to_owned())
}

/// Starts a GUI program with `args` exactly as typed, working folder %USERPROFILE%.
pub fn start_program(path: &str, args: &str) -> std::io::Result<()> {
    let params = (!args.is_empty()).then_some(args);
    open_with(path, params, home().as_deref())
}

/// PowerShell reads a line that opens with a quote as a string, not a program (a quoted path with args)
/// is an error. The call operator makes it run, which is what the Owner meant.
fn callable(line: &str) -> std::borrow::Cow<'_, str> {
    if line.starts_with(['"', '\'']) {
        format!("& {line}").into()
    } else {
        line.into()
    }
}

/// Base64 (RFC 4648, padded) of the UTF-16LE bytes of `line`: PowerShell's -EncodedCommand.
pub fn encoded_command(line: &str) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = line.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |a, (i, b)| a | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// From the first bytes of a file: Some(true) for PE subsystem 2 (WINDOWS_GUI), Some(false) for any
/// other PE subsystem, None when it is not a PE image.
pub fn subsystem_is_gui(head: &[u8]) -> Option<bool> {
    let u16_at = |at: usize| -> Option<u16> {
        let end = at.checked_add(2)?;
        Some(u16::from_le_bytes(head.get(at..end)?.try_into().ok()?))
    };
    let e_lfanew = u32::from_le_bytes(head.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if head.get(e_lfanew..e_lfanew.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let opt = e_lfanew.checked_add(24)?;
    if !matches!(u16_at(opt)?, 0x10B | 0x20B) {
        return None;
    }
    Some(u16_at(opt.checked_add(68)?)? == 2)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

fn is_path_token(token: &str) -> bool {
    token.contains(['\\', '/', ':'])
}

/// `X:\` or `X:/` start: the only form looked at. UNC, `\\?\`, rooted and relative forms are not.
fn is_drive_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/')
}

/// True for anything that is not a drive-absolute path on a non-remote drive.
fn is_remote(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if !is_drive_absolute(&s) {
        return true;
    }
    let root = HSTRING::from(format!("{}:\\", s[..1].to_ascii_uppercase()));
    // SAFETY: a plain query on a drive root string that lives for the call; it does not touch the
    // network.
    unsafe { GetDriveTypeW(&root) == DRIVE_REMOTE }
}

/// Files to try for `token`, in order. Directories and tokens that are not drive-absolute (UNC
/// among them) yield nothing, so no UNC path is ever tried (T-035).
fn candidates(token: &str, path_var: &OsStr, pathext: &str) -> Vec<PathBuf> {
    let exts: Vec<&str> = pathext.split(';').filter(|e| !e.is_empty()).collect();
    let expand = |base: PathBuf, as_is: bool| -> Vec<PathBuf> {
        let ext = base.extension().and_then(OsStr::to_str);
        // `app.v2` has a dot but no program extension, so `app.v2.exe` is still a candidate.
        let runnable = ext.is_some_and(|x| {
            exts.iter()
                .any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(x))
        });
        let mut v = Vec::new();
        if base.extension().is_some() || as_is {
            v.push(base.clone());
        }
        if !runnable {
            for e in &exts {
                let mut s = base.clone().into_os_string();
                s.push(e);
                v.push(PathBuf::from(s));
            }
        }
        v
    };
    if is_path_token(token) {
        return if is_drive_absolute(token) {
            expand(PathBuf::from(token), true)
        } else {
            Vec::new()
        };
    }
    std::env::split_paths(path_var)
        .filter(|d| is_drive_absolute(&d.to_string_lossy()))
        .flat_map(|d| expand(d.join(token), false))
        .collect()
}

/// Some(Found) when `p` is a file on a non-remote drive; `gui` from its PE header.
fn probe(p: &Path) -> Option<Found> {
    if is_remote(p) {
        return None;
    }
    // symlink_metadata: an App Execution Alias exists but cannot be opened.
    if !std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir()) {
        return None;
    }
    let is_exe = p
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("com"));
    let gui = is_exe && read_head(p).and_then(|h| subsystem_is_gui(&h)) == Some(true);
    Some(Found {
        path: p.to_string_lossy().into_owned(),
        gui,
    })
}

fn read_head(p: &Path) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    std::fs::File::open(p)
        .ok()?
        .take(HEAD_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// The default value of `App Paths\<name>` under `root`: env expanded, quotes trimmed.
fn app_path(root: HKEY, name: &str) -> Option<String> {
    let key = HSTRING::from(format!("{APP_PATHS}{name}"));
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `len` bytes and RegGetValueW writes no more than that; REG_EXPAND_SZ is
    // expanded by the call (no RRF_NOEXPAND).
    let err = unsafe {
        RegGetValueW(
            root,
            &key,
            None,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            Some(&mut len),
        )
    };
    if err != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    let s = String::from_utf16(&buf[..chars]).ok()?;
    let s = s.trim().trim_matches('"').to_string();
    (!s.is_empty()).then_some(s)
}

/// pwsh.exe when `find` resolves it, else powershell.exe, else None.
fn pick_shell(find: impl Fn(&str) -> Option<Found>) -> Option<String> {
    find("pwsh.exe")
        .or_else(|| find("powershell.exe"))
        .map(|f| f.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe(magic: u16, subsystem: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x98..0x9A].copy_from_slice(&magic.to_le_bytes());
        b[0x98 + 68..0x98 + 70].copy_from_slice(&subsystem.to_le_bytes());
        b
    }

    // @R-024
    #[test]
    fn r024_a_line_that_starts_with_a_quote_gets_the_call_operator() {
        assert_eq!(callable(r#""C:\a b\x.exe" -v"#), r#"& "C:\a b\x.exe" -v"#);
        assert_eq!(callable("'x' y"), "& 'x' y");
        assert_eq!(callable("ping a"), "ping a");
        assert_eq!(callable("Get-Date"), "Get-Date");
    }

    // @R-024
    #[test]
    fn r024_encoded_command_is_utf16le_base64() {
        assert_eq!(encoded_command("ping"), "cABpAG4AZwA=");
        assert_eq!(encoded_command("é"), "6QA=");
        assert_eq!(encoded_command(""), "");
    }

    // @R-024
    #[test]
    fn r024_pe_subsystem_gui_and_console() {
        assert_eq!(subsystem_is_gui(&pe(0x10B, 2)), Some(true));
        assert_eq!(subsystem_is_gui(&pe(0x20B, 2)), Some(true));
        assert_eq!(subsystem_is_gui(&pe(0x10B, 3)), Some(false));
        assert_eq!(subsystem_is_gui(&pe(0x20B, 3)), Some(false));
        assert_eq!(subsystem_is_gui(&pe(0x20B, 2)[..0x90]), None);
        assert_eq!(subsystem_is_gui(&pe(0x20B, 2)[..0x20]), None);
        assert_eq!(subsystem_is_gui(&pe(0x999, 2)), None);
        let mut far = pe(0x20B, 2);
        far[0x3C..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(subsystem_is_gui(&far), None);
        let mut no_sig = pe(0x20B, 2);
        no_sig[0x80] = b'X';
        assert_eq!(subsystem_is_gui(&no_sig), None);
        assert_eq!(subsystem_is_gui(&[]), None);

        assert!(find_program("notepad").is_some_and(|f| f.gui));
        assert!(find_program("cmd").is_some_and(|f| !f.gui));
        assert_eq!(find_program("naobi-no-such-program-xyz"), None);
    }

    // @R-024
    #[test]
    fn r024_lookup_skips_unc_path_dirs() {
        let path = OsStr::new(r"\\server\share;C:\Tools;//other/share");
        let c = candidates("tool", path, ".EXE;.CMD");
        assert_eq!(
            c,
            vec![
                PathBuf::from(r"C:\Tools\tool.EXE"),
                PathBuf::from(r"C:\Tools\tool.CMD")
            ]
        );
        assert!(candidates(r"\\server\share\x.exe", path, ".EXE").is_empty());
        assert!(candidates("//server/share/x.exe", path, ".EXE").is_empty());
        assert!(candidates(r"rel\x.exe", path, ".EXE").is_empty());
        assert_eq!(
            candidates(r"C:\a\x", OsStr::new(""), ".EXE"),
            vec![PathBuf::from(r"C:\a\x"), PathBuf::from(r"C:\a\x.EXE")]
        );
        // A dot that is no program extension still gets PATHEXT; a real one does not.
        assert_eq!(
            candidates("app.v2", OsStr::new(r"C:\Tools"), ".EXE"),
            vec![
                PathBuf::from(r"C:\Tools\app.v2"),
                PathBuf::from(r"C:\Tools\app.v2.EXE")
            ]
        );
        assert_eq!(
            candidates("app.exe", OsStr::new(r"C:\Tools"), ".EXE"),
            vec![PathBuf::from(r"C:\Tools\app.exe")]
        );
    }

    // @R-024
    #[test]
    fn r024_console_falls_back_to_windows_powershell() {
        let f = |p: &str| {
            Some(Found {
                path: p.into(),
                gui: false,
            })
        };
        assert_eq!(pick_shell(f), Some("pwsh.exe".into()));
        assert_eq!(
            pick_shell(|n| if n == "powershell.exe" { f(n) } else { None }),
            Some("powershell.exe".into())
        );
        assert_eq!(pick_shell(|_| None), None);
    }
}
