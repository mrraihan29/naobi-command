//! Row icons (NCMD-16, REPORT 3.6): loaded on demand by a transient child of this exe (`--icons`),
//! because the shell libraries an icon needs stay mapped for good and would break the idle budget
//! (R-006). The parent asks by key only (`a:<app id>`, `e:<extension>`; never a path, T-030) and
//! each picture goes back to the controller as it arrives. The cache of 100 is the controller's.
use naobi_core::ui::Icon;
use naobi_launcher::icons::askable;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The argument that makes the exe answer icon keys on stdin and stdout until stdin closes.
pub const ICONS_ARG: &str = "--icons";
/// Nobody asked for this long: the child (and the shell libraries in it) goes away.
const IDLE: Duration = Duration::from_secs(30);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The largest picture the parent accepts from the child.
const MAX_SIDE: u32 = 256;

/// Asks for the icons of `keys`. Returns at once; each icon arrives on the UI thread.
pub fn request(keys: Vec<String>) {
    static WORKER: OnceLock<Mutex<Sender<Vec<String>>>> = OnceLock::new();
    let tx = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Vec<String>>();
        let spawned = std::thread::Builder::new()
            .name("icons".into())
            .spawn(move || {
                let mut child: Option<Running> = None;
                loop {
                    let batch = if child.is_some() {
                        match rx.recv_timeout(IDLE) {
                            Ok(b) => b,
                            Err(RecvTimeoutError::Timeout) => {
                                child = None;
                                continue;
                            }
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    } else {
                        match rx.recv() {
                            Ok(b) => b,
                            Err(_) => return,
                        }
                    };
                    if child.is_none() {
                        child = Running::start().ok();
                    }
                    let sent = child.as_mut().is_some_and(|c| c.send(&batch));
                    if !sent {
                        // The child is gone or never started: these keys have no icon.
                        child = None;
                        let none: Vec<(String, Option<Icon>)> =
                            batch.into_iter().map(|k| (k, None)).collect();
                        crate::to_modules(move |c, h| c.on_icons(none, h));
                    }
                }
            });
        if spawned.is_err() {
            tracing::warn!("icons are off: their thread could not start");
        }
        Mutex::new(tx)
    });
    let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(keys);
}

/// The running child: its stdin. A reader thread turns each answer line into an `on_icons` call.
struct Running {
    child: Child,
    stdin: ChildStdin,
}

impl Running {
    fn start() -> std::io::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(ICONS_ARG)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(std::io::Error::other("the icon child has no pipes"));
        };
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(answer) = parse_answer(&line) {
                    crate::to_modules(move |c, h| c.on_icons(vec![answer], h));
                }
            }
        });
        Ok(Self { child, stdin })
    }

    fn send(&mut self, keys: &[String]) -> bool {
        keys.iter()
            .filter(|k| askable(k))
            .try_for_each(|k| writeln!(self.stdin, "{k}"))
            .and_then(|()| self.stdin.flush())
            .is_ok()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `key<TAB>-` (no icon) or `key<TAB>WxH<TAB>hex`; anything else is not an answer.
pub fn parse_answer(line: &str) -> Option<(String, Option<Icon>)> {
    let mut parts = line.split('\t');
    let key = parts.next().filter(|k| askable(k))?;
    let size = parts.next()?;
    if size == "-" {
        return Some((key.to_owned(), None));
    }
    let (w, h) = size.split_once('x')?;
    let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let rgba = from_hex(parts.next()?)?;
    (rgba.len() == (w * h * 4) as usize && parts.next().is_none())
        .then(|| (key.to_owned(), Some(Icon { w, h, rgba })))
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(DIGITS[usize::from(b >> 4)]));
        s.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    s
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    bytes
        .chunks(2)
        .map(|p| Some(digit(p[0])? << 4 | digit(p[1])?))
        .collect()
}

/// The child: reads one key per line and answers each. Ends when stdin closes.
pub fn icons() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    naobi_win32::icons::with_com(|| -> anyhow::Result<()> {
        for line in stdin.lock().lines() {
            let key = line?;
            // The parent checked it; the child does not trust the pipe either.
            let icon = if !askable(&key) {
                None
            } else if let Some(id) = key.strip_prefix("a:") {
                naobi_win32::icons::app_icon(id)
            } else {
                key.strip_prefix("e:")
                    .and_then(naobi_win32::icons::ext_icon)
            };
            match icon {
                Some(i) => writeln!(out, "{key}\t{}x{}\t{}", i.w, i.h, to_hex(&i.rgba))?,
                None => writeln!(out, "{key}\t-")?,
            }
            out.flush()?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-012
    #[test]
    fn r012_an_icon_answer_round_trips_and_a_bad_one_is_refused() {
        let rgba = vec![1, 2, 3, 255, 4, 5, 6, 128];
        let line = format!("a:Contoso!App\t2x1\t{}", to_hex(&rgba));
        let (key, icon) = parse_answer(&line).unwrap_or_default();
        assert_eq!(key, "a:Contoso!App");
        assert_eq!(icon, Some(Icon { w: 2, h: 1, rgba }));
        assert_eq!(parse_answer("e:txt\t-"), Some(("e:txt".to_owned(), None)));
        for bad in [
            "",
            "e:txt",
            "e:txt\t2x1",
            "e:txt\t2x1\t0102",
            "e:txt\t0x1\t",
            "e:txt\t1x1\t0102030g",
            "e:txt\t1x1\t01020304\textra",
            "e:txt\t9999x1\t00",
            "a:\\\\evil\\x\t-",
            "x:y\t-",
            "e:txt\t1x1\t010203",
        ] {
            assert_eq!(parse_answer(bad), None, "{bad:?}");
        }
    }
}
