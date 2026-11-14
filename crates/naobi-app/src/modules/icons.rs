//! Row icons (NCMD-16, REPORT 3.6): loaded on demand by a transient child of this exe (`--icons`),
//! because the shell libraries an icon needs stay mapped for good and would break the idle budget
//! (R-006). The parent asks by key only (`a:<app id>`, `e:<extension>`; never a path, T-030) and
//! each batch of pictures goes back to the controller. The cache of 100 is the controller's.
//!
//! One worker thread owns the child. A writer thread feeds its stdin and a reader thread turns its
//! stdout into messages, so the worker never blocks on a stuck child: no answer for `STUCK` and the
//! child is killed, its keys are answered "try again", and the next request starts a new one.
use naobi_core::ui::Icon;
use naobi_launcher::icons::{askable, Answer};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The argument that makes the exe answer icon keys on stdin and stdout until stdin closes.
pub const ICONS_ARG: &str = "--icons";
/// Nobody asked for this long: the child (and the shell libraries in it) goes away.
const IDLE: Duration = Duration::from_secs(30);
/// An asked key with no answer at all for this long: the child is stuck (a hung icon handler).
const STUCK: Duration = Duration::from_secs(5);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The largest picture the parent accepts from the child.
const MAX_SIDE: u32 = 256;
/// The longest answer line: an icon in hex is 16 KB.
const MAX_LINE: u64 = 64 * 1024;

/// What the worker hears about: requests, and what the child says.
enum Msg {
    Keys(Vec<String>),
    /// `(run, key, icon)`: an answer from child number `run`; `None` is "this key has no icon".
    Answer(u64, String, Option<Icon>),
    /// Child number `run` closed its stdout.
    Dead(u64),
}

/// Asks for the icons of `keys`. Returns at once; the answers arrive on the UI thread.
pub fn request(keys: Vec<String>) {
    static WORKER: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();
    let tx = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Msg>();
        let own = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("icons".into())
            .spawn(move || work(&rx, &own));
        if spawned.is_err() {
            tracing::warn!("icons are off: their thread could not start");
        }
        Mutex::new(tx)
    });
    let _ = tx
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(Msg::Keys(keys));
}

fn deliver(found: Vec<(String, Answer)>) {
    if !found.is_empty() {
        crate::to_modules(move |c, h| c.on_icons(found, h));
    }
}

fn answer_of(icon: Option<Icon>) -> Answer {
    icon.map_or(Answer::Missing, Answer::Icon)
}

fn work(rx: &Receiver<Msg>, own: &Sender<Msg>) {
    let mut child: Option<Running> = None;
    let mut runs = 0u64;
    // Keys written and not yet answered, and when the child last said anything.
    let mut pending: HashSet<String> = HashSet::new();
    let mut heard = Instant::now();
    loop {
        // A busy child is watched every 250 ms, a quiet one until idle, none at all just waits.
        let got = match (&child, pending.is_empty()) {
            (Some(_), false) => rx.recv_timeout(Duration::from_millis(250)),
            (Some(_), true) => rx.recv_timeout(IDLE),
            (None, _) => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let mut found: Vec<(String, Answer)> = Vec::new();
        match got {
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                if pending.is_empty() {
                    // Idle: let the child go.
                    child = None;
                } else if heard.elapsed() > STUCK {
                    child = None;
                    found.extend(pending.drain().map(|k| (k, Answer::Retry)));
                }
            }
            Ok(Msg::Keys(keys)) => {
                if child.is_none() {
                    runs += 1;
                    child = Running::start(runs, own.clone()).ok();
                    heard = Instant::now();
                    if child.is_none() {
                        tracing::warn!("the icon child did not start");
                    }
                }
                for key in keys.into_iter().filter(|k| askable(k)) {
                    let sent = child.as_ref().is_some_and(|c| c.send(&key));
                    if sent {
                        if pending.is_empty() {
                            heard = Instant::now();
                        }
                        pending.insert(key);
                    } else {
                        found.push((key, Answer::Retry));
                    }
                }
            }
            Ok(Msg::Answer(run, key, icon)) => {
                if child.as_ref().is_some_and(|c| c.run == run) {
                    heard = Instant::now();
                    if pending.remove(&key) {
                        found.push((key, answer_of(icon)));
                    }
                    // Whatever else is already here goes out in the same batch.
                    while let Ok(next) = rx.try_recv() {
                        match next {
                            Msg::Answer(r, k, i) if r == run && pending.remove(&k) => {
                                found.push((k, answer_of(i)));
                            }
                            // Requests and deaths wait for the next turn of the loop.
                            other => {
                                let _ = own.send(other);
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Msg::Dead(run)) => {
                if child.as_ref().is_some_and(|c| c.run == run) {
                    child = None;
                    found.extend(pending.drain().map(|k| (k, Answer::Retry)));
                }
            }
        }
        deliver(found);
    }
}

/// The running child: its number, the way to write to it, and the process.
struct Running {
    run: u64,
    keys: Sender<String>,
    child: Child,
}

impl Running {
    fn start(run: u64, to_worker: Sender<Msg>) -> std::io::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(ICONS_ARG)
            .creation_flags(CREATE_NO_WINDOW)
            // Nothing relative may resolve to a file in a folder the Owner happened to start from.
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(std::io::Error::other("the icon child has no pipes"));
        };
        // The writer may block on a full pipe when the child is stuck; killing the child frees it.
        let (keys, to_child) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for key in to_child {
                if writeln!(stdin, "{key}")
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                // A line that never ends is cut at `MAX_LINE`, which is not a whole line: stop.
                match (&mut reader).take(MAX_LINE).read_line(&mut line) {
                    Ok(n) if n > 0 && line.ends_with('\n') => {}
                    _ => break,
                }
                if let Some((key, icon)) = parse_answer(line.trim_end_matches(['\r', '\n'])) {
                    if to_worker.send(Msg::Answer(run, key, icon)).is_err() {
                        return;
                    }
                }
            }
            let _ = to_worker.send(Msg::Dead(run));
        });
        Ok(Self { run, keys, child })
    }

    fn send(&self, key: &str) -> bool {
        self.keys.send(key.to_owned()).is_ok()
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
