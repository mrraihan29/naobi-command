//! `hotkey-to-visible` (R-006): t0 = QPC just before SendInput of Alt+Space, t1 = first poll where a pixel
//! inside the Launcher rectangle of the composed desktop equals the Launcher background color.
//! Also holds what both latency metrics share: the child exe, the pinned conditions, the backdrop.
use crate::keystroke::parse_line;
use crate::percentile::percentile;
use crate::verdict::{record, verdict};
use anyhow::{anyhow, bail, Result};
use naobi_win32::platform::{qpc, qpc_to_ms};
use naobi_win32::probe::{self, Backdrop, Screen};
use naobi_win32::{dwm, inject, process::ProcessMetrics};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// The refresh rate the verdict is pinned to: at 60 Hz the vsync period alone would exceed 16 ms.
pub const PINNED_HZ: u32 = 143;

/// GDI reports an integer: the Owner's 143 Hz panel mode (143.998 Hz) reads as 144.
pub fn pinned_rate(hz: u32) -> bool {
    hz == PINNED_HZ || hz == PINNED_HZ + 1
}
const POLL_TIMEOUT: Duration = Duration::from_secs(2);

/// Launcher background (`Tokens.window`): paper #FAF8F2, stone #2A2622.
pub fn launcher_bg() -> u32 {
    if dwm::system_uses_dark_theme() {
        probe::colorref(0x2A, 0x26, 0x22)
    } else {
        probe::colorref(0xFA, 0xF8, 0xF2)
    }
}

/// The exe built with feature `perf`, as a child with its stdout lines parsed on a thread.
pub struct App {
    pub child: Child,
    pub lines: Receiver<(String, i64)>,
    /// Throwaway data and settings folders, so a run never touches the Owner's `naobi.db` or
    /// `settings.toml` (the perf build writes no Run value without an override).
    sandbox: PathBuf,
}

impl App {
    pub fn spawn(exe: &Path, items: usize) -> Result<App> {
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let sandbox = std::env::temp_dir().join(format!(
            "naobi-perf-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        let mut child = Command::new(exe)
            .env("NAOBI_PERF_ITEMS", items.to_string())
            .env("NAOBI_DATA_DIR", sandbox.join("data"))
            .env("NAOBI_SETTINGS_DIR", sandbox.join("settings"))
            .stdout(Stdio::piped())
            .spawn()?;
        let out = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("no stdout pipe"))?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(|l| l.ok()) {
                if let Some(ev) = parse_line(&line) {
                    let _ = tx.send(ev);
                }
            }
        });
        Ok(App {
            child,
            lines,
            sandbox,
        })
    }

    /// Waits for the readiness line `ev`; returns its QPC ticks. Fails if the exe exits (e.g. Alt+Space is taken).
    pub fn wait(&mut self, ev: &str) -> Result<i64> {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if self.child.try_wait()?.is_some() {
                bail!("ENV: the exe exited before {ev} (is Alt+Space taken by another process?)");
            }
            let left = end.saturating_duration_since(Instant::now());
            match self
                .lines
                .recv_timeout(left.min(Duration::from_millis(200)))
            {
                Ok((name, q)) if name == ev => return Ok(q),
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < end => {}
                _ => bail!("no {ev} line within 10 s"),
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.sandbox);
    }
}

fn running(image: &str) -> bool {
    Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {image}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(image))
        .unwrap_or(false)
}

/// Refuses to start unless the pinned conditions that can be checked before the run hold.
pub fn check_env() -> Result<()> {
    if cfg!(debug_assertions) {
        bail!("ENV: a verdict needs a release build (cargo run --release)");
    }
    // The previous run's app was just killed; its transient children (`--icons`, `--search-files`) may
    // need a moment to go. A dogfood instance stays, so it still ends the run after the wait.
    let end = Instant::now() + Duration::from_secs(8);
    while running("naobi-app.exe") && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(250));
    }
    for img in ["naobi-app.exe", "PowerToys.PowerLauncher.exe"] {
        if running(img) {
            bail!("ENV: {img} is running; quit it first");
        }
    }
    if !probe::on_ac_power() {
        bail!("ENV: not on AC power");
    }
    Ok(())
}

/// Mean CPU of everything except the harness and the exe, over the window between `start` and `finish`.
pub struct BgCpu {
    sys: (u64, u64),
    own: u64,
}

fn own_cpu(pid: u32) -> u64 {
    let me = ProcessMetrics::sample(std::process::id()).map_or(0, |m| m.cpu_100ns);
    me + ProcessMetrics::sample(pid).map_or(0, |m| m.cpu_100ns)
}

impl BgCpu {
    pub fn start(pid: u32) -> BgCpu {
        BgCpu {
            sys: probe::system_cpu(),
            own: own_cpu(pid),
        }
    }

    pub fn finish(self, pid: u32) -> f64 {
        let (busy, total) = probe::system_cpu();
        let own = own_cpu(pid).saturating_sub(self.own);
        let busy = (busy - self.sys.0).saturating_sub(own);
        busy as f64 / (total - self.sys.1).max(1) as f64 * 100.0
    }
}

/// One run: `fire` sends the input, `pixel` samples the composed desktop, `now` is QPC ticks.
/// `None` when the backdrop precondition fails (run discarded) or the pixel never reaches `bg`.
pub fn run_once(
    mut pixel: impl FnMut() -> u32,
    bg: u32,
    fire: impl FnOnce(),
    now: impl Fn() -> i64,
    timeout: Duration,
) -> Option<i64> {
    if pixel() == bg {
        return None;
    }
    let t0 = now();
    fire();
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if pixel() == bg {
            return Some(now() - t0);
        }
    }
    None
}

/// Shows or hides the Launcher with Alt+Space until `cond` holds, so a run starts from a known state.
fn settle(mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + POLL_TIMEOUT;
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

/// Pins the conditions, launches `exe`, and returns the app, its Launcher rectangle and the helper backdrop.
/// The Launcher has been shown once (so it has its position) and is cloaked again.
/// Screen rectangle `(x, y, w, h)`.
pub type Rect = (i32, i32, i32, i32);

pub fn prepare(exe: &Path, items: usize) -> Result<(App, Rect, Backdrop)> {
    check_env()?;
    probe::dpi_aware();
    let mut app = App::spawn(exe, items)?;
    app.wait("tray_ready")?;
    app.wait("window_ready")?;
    let hwnd = dwm::find_window(naobi_ui_title()).ok_or_else(|| anyhow!("no Launcher window"))?;
    inject::alt_space();
    if !settle(|| !dwm::is_cloaked(hwnd)) {
        bail!("warm-up show never happened");
    }
    std::thread::sleep(Duration::from_millis(300));
    let rect =
        probe::window_rect(naobi_ui_title()).ok_or_else(|| anyhow!("no Launcher rectangle"))?;
    inject::alt_space();
    if !settle(|| dwm::is_cloaked(hwnd)) {
        bail!("warm-up hide never happened");
    }
    let hz = probe::refresh_hz(rect);
    if !pinned_rate(hz) {
        bail!("ENV: display refresh is {hz} Hz, the verdict is pinned to {PINNED_HZ} Hz");
    }
    let backdrop = Backdrop::new(rect)?;
    backdrop.pump();
    std::thread::sleep(Duration::from_millis(300));
    Ok((app, rect, backdrop))
}

/// Title of the Launcher window (naobi-ui::TITLE; the harness must not link Slint).
pub fn naobi_ui_title() -> &'static str {
    "Naobi Command Launcher"
}

/// p50, p95 and max of `v` (ms).
pub fn stats(v: &[f64]) -> Value {
    let p = |q| percentile(v, q).unwrap_or(f64::NAN);
    json!({ "p50": p(50.0), "p95": p(95.0), "max": p(100.0), "n": v.len() })
}

/// Common JSON block: every condition the verdict is valid under.
pub fn conditions(rect: (i32, i32, i32, i32), bg_cpu: f64, exe: &Path) -> Value {
    json!({
        "machine": std::env::var("COMPUTERNAME").unwrap_or_default(),
        "refresh_hz": probe::refresh_hz(rect),
        "pinned_hz": PINNED_HZ,
        "vsync_period_ms": 1000.0 / probe::refresh_hz(rect).max(1) as f64,
        "ac_power": probe::on_ac_power(),
        "harness_release_build": !cfg!(debug_assertions),
        "exe": exe.display().to_string(),
        "dogfood_and_powertoys_run_absent": true,
        "background_cpu_percent": bg_cpu,
        "background_cpu_note": "recorded only, no limit (D-036)",
        "launcher_rect": rect,
    })
}

pub fn run(exe: &Path, runs: usize, out: &Path) -> Result<()> {
    let (app, rect, _backdrop) = prepare(exe, 10_000)?;
    let pid = app.child.id();
    let hwnd = dwm::find_window(naobi_ui_title()).ok_or_else(|| anyhow!("no Launcher window"))?;
    let (x, y, w, h) = rect;
    let (px, py) = (x + w / 2, y + h / 20);
    let (screen, bg) = (Screen::new((8, 8)), launcher_bg());
    let (mut ms, mut discarded) = (Vec::new(), 0);
    let cpu = BgCpu::start(pid);
    let mut attempts = 0;
    while ms.len() < runs && attempts < runs * 2 {
        attempts += 1;
        // Backdrop precondition: the pixel under the cloaked Launcher must differ from its background.
        let t = run_once(
            || screen.pixel(px, py),
            bg,
            inject::alt_space,
            qpc,
            POLL_TIMEOUT,
        );
        match t {
            Some(ticks) => ms.push(qpc_to_ms(ticks)),
            None => discarded += 1,
        }
        inject::alt_space();
        if !settle(|| dwm::is_cloaked(hwnd)) {
            bail!("hide never happened");
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    let bg_cpu = cpu.finish(pid);
    if ms.len() < runs {
        bail!(
            "only {} of {runs} valid runs ({discarded} discarded)",
            ms.len()
        );
    }
    // Probe cost of one pixel sample, for the record.
    let t = Instant::now();
    for _ in 0..200 {
        screen.pixel(px, py);
    }
    let pixel_cost_ms = t.elapsed().as_secs_f64() * 1000.0 / 200.0;
    let s = stats(&ms);
    // D-036: background CPU is recorded, never a reason to refuse a verdict.
    let valid = true;
    let mut v = verdict(
        "hotkey_to_visible_p95",
        s["p95"].as_f64().unwrap_or(f64::NAN),
        50.0,
        "ms",
    );
    v["pass"] = json!(v["pass"] == true && valid);
    v["inject_to_t1_ms"] = s;
    v["discarded_runs"] = json!(discarded);
    v["probe_pixel_cost_ms"] = json!(pixel_cost_ms);
    v["conditions"] = conditions(rect, bg_cpu, exe);
    v["conditions_valid"] = json!(valid);
    println!("{v}");
    record(out, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const BG: u32 = 0x00F2_F8FA;

    // @R-006
    #[test]
    fn r006_probe_requires_contrasting_backdrop() {
        // Synthetic frame source: the pixel already equals the Launcher background before the show.
        let fired = Cell::new(false);
        let r = run_once(
            || BG,
            BG,
            || fired.set(true),
            || 0,
            Duration::from_millis(50),
        );
        assert_eq!(r, None, "run must be discarded");
        assert!(
            !fired.get(),
            "no input may be sent without a contrasting backdrop"
        );

        // Backdrop present, then the Launcher paints: the run measures.
        let shown = Cell::new(false);
        let ticks = Cell::new(0i64);
        let r = run_once(
            || if shown.get() { BG } else { probe::BACKDROP },
            BG,
            || shown.set(true),
            || {
                ticks.set(ticks.get() + 5);
                ticks.get()
            },
            Duration::from_millis(50),
        );
        assert_eq!(r, Some(5), "t1 - t0 from the synthetic clock");
    }
}
