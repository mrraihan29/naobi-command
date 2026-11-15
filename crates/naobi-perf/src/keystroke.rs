//! `keystroke-to-results` (R-006 v4, RK-15): t0 = QPC in the window subclass procedure at the key's first
//! `WM_KEYDOWN`/`WM_CHAR`; t1 = end of the first probe iteration whose composed-desktop hash of the results
//! list differs from the pre-key hash. `p95_est` = `p95_upper` - probe cost.
use crate::hotkey::{self, launcher_bg, stats, BgCpu};
use crate::percentile::percentile;
use crate::verdict::{record, verdict};
use anyhow::{bail, Result};
use naobi_win32::inject;
use naobi_win32::platform::{qpc, qpc_to_ms};
use naobi_win32::probe::{self, Screen};
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};

/// Logical size of the hashed list region, relative to the Launcher: 10 px in, below the search row,
/// the first three result rows (40 px each).
const REGION: (i32, i32, i32, i32) = (10, 68, 400, 120);
const KEY_TIMEOUT: Duration = Duration::from_secs(1);

/// Parses one stdout line `{"ev":"key","qpc":123}` into `(ev, qpc)`.
pub fn parse_line(line: &str) -> Option<(String, i64)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    Some((v["ev"].as_str()?.to_string(), v["qpc"].as_i64()?))
}

/// The injected key lines must be exactly `n`, strictly increasing in QPC.
pub fn check_key_lines(lines: &[(String, i64)], n: usize) -> Result<()> {
    let keys: Vec<i64> = lines.iter().filter(|l| l.0 == "key").map(|l| l.1).collect();
    if keys.len() != n {
        bail!("{} key lines for {n} keys", keys.len());
    }
    if keys.windows(2).any(|w| w[1] <= w[0]) {
        bail!("key lines out of order");
    }
    Ok(())
}

/// Keystroke bar of R-006 v5 / D-037: `p95_est` (p95_upper minus the measured probe cost) <= 25 ms on screen.
/// The true on-screen p95 lies between `p95_est` (assumes the whole probe wait is overhead) and `p95_upper`.
/// The parity part (inject_to_t1 no worse than PowerToys Command Palette) is judged by NCMD-6, not here.
pub fn decide(p95_est: f64) -> &'static str {
    if p95_est <= 25.0 {
        "PASS"
    } else {
        "FAIL"
    }
}

/// Key `i` of the run: a letter, Backspace, the other letter, Backspace...
fn key(i: usize) -> u16 {
    const VK_BACK: u16 = 0x08;
    match (i % 2, i / 2 % 2) {
        (1, _) => VK_BACK,
        (_, 0) => u16::from(b'X'),
        _ => u16::from(b'C'),
    }
}

pub fn run(exe: &Path, runs: usize, items: usize, out: &Path) -> Result<()> {
    let (app, rect, _backdrop) = hotkey::prepare(exe, items)?;
    let pid = app.child.id();
    let (x, y, w, h) = rect;
    let s = w as f64 / 750.0;
    let sc = |v: i32| (v as f64 * s).round() as i32;
    let (rx, ry) = (x + sc(REGION.0), y + sc(REGION.1));
    let mut screen = Screen::new((sc(REGION.2), sc(REGION.3)));
    let (px, py, bg) = (x + w / 2, y + h / 20, launcher_bg());

    // Precondition before the show: the pixel under the cloaked Launcher must differ from its background.
    if screen.pixel(px, py) == bg {
        bail!("backdrop precondition failed: the pixel under the Launcher already equals its background");
    }
    let cpu = BgCpu::start(pid);
    inject::launcher();
    let end = Instant::now() + Duration::from_secs(2);
    while screen.pixel(px, py) != bg {
        if Instant::now() > end {
            bail!("Launcher never became visible");
        }
    }
    std::thread::sleep(Duration::from_millis(500));

    // Probe cost: 200 iterations on a static frame.
    let mut cost = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        screen.region_hash(rx, ry);
        cost.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let probe_cost = percentile(&cost, 50.0).unwrap_or(f64::NAN);
    let probe_spread = percentile(&cost, 95.0).unwrap_or(f64::NAN) - probe_cost;

    let (mut upper, mut from_inject, mut to_wndproc) = (Vec::new(), Vec::new(), Vec::new());
    let (mut discarded, mut keys, mut seen) = (0, 0, Vec::new());
    let mut attempts = 0;
    while upper.len() < runs && attempts < runs * 2 {
        let vk = key(attempts);
        attempts += 1;
        std::thread::sleep(Duration::from_millis(80));
        while let Ok(l) = app.lines.try_recv() {
            seen.push(l);
        }
        let pre = screen.region_hash(rx, ry);
        let t_inject = qpc();
        probe::press(vk);
        keys += 1;
        let start = Instant::now();
        let t1 = loop {
            if screen.region_hash(rx, ry) != pre {
                break Some(qpc());
            }
            if start.elapsed() > KEY_TIMEOUT {
                break None;
            }
        };
        let line = app.lines.recv_timeout(Duration::from_millis(500)).ok();
        match (t1, line) {
            (Some(t1), Some((ev, t0))) if ev == "key" && t_inject <= t0 && t0 <= t1 => {
                upper.push(qpc_to_ms(t1 - t0));
                from_inject.push(qpc_to_ms(t1 - t_inject));
                to_wndproc.push(qpc_to_ms(t0 - t_inject));
                seen.push((ev, t0));
            }
            _ => discarded += 1,
        }
    }
    let bg_cpu = cpu.finish(pid);
    inject::launcher();
    if upper.len() < runs {
        bail!(
            "only {} of {runs} valid runs ({discarded} discarded)",
            upper.len()
        );
    }
    check_key_lines(&seen, upper.len())?;
    let p95_upper = percentile(&upper, 95.0).unwrap_or(f64::NAN);
    let p95_est = p95_upper - probe_cost;
    let valid = hotkey::pinned_rate(probe::refresh_hz(rect));
    let mut v = verdict("keystroke_to_results_p95_est", p95_est, 25.0, "ms");
    v["pass"] = json!(v["pass"] == true && valid);
    v["decision"] = json!(decide(p95_est));
    v["p95_upper"] = json!(p95_upper);
    v["bar"] = json!("R-006 v5 / D-037: p95_est <= 25 ms; parity with PowerToys Command Palette inject_to_t1 pending NCMD-6");
    v["probe_cost_ms"] = json!(probe_cost);
    v["probe_p95_minus_p50_ms"] = json!(probe_spread);
    v["wndproc_to_t1_ms"] = stats(&upper);
    v["inject_to_t1_ms"] = stats(&from_inject);
    v["inject_to_wndproc_ms"] = stats(&to_wndproc);
    v["items"] = json!(items);
    v["keys_sent"] = json!(keys);
    v["discarded_runs"] = json!(discarded);
    v["probe_region_px"] = json!([sc(REGION.2), sc(REGION.3)]);
    v["conditions"] = hotkey::conditions(rect, bg_cpu, exe);
    v["conditions_valid"] = json!(valid);
    v["judged_on"] = json!("p95_est; p95_upper is the conservative bound");
    v["on_screen_p95_range_ms"] = json!([p95_est, p95_upper]);
    println!(
        "keystroke-to-results: bar 25 ms judged on p95_est = {p95_est:.1} ms (conservative bound p95_upper = {p95_upper:.1} ms, probe cost {probe_cost:.1} ms); the true on-screen p95 lies between them: {}",
        decide(p95_est)
    );
    println!("{v}");
    record(out, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(q: i64) -> (String, i64) {
        ("key".into(), q)
    }

    #[test]
    fn d021_key_lines_parsed_and_ordered() {
        assert_eq!(
            parse_line(r#"{"ev":"key","qpc":42}"#),
            Some(("key".to_string(), 42))
        );
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"ev":"key"}"#), None);
        assert!(check_key_lines(&[k(1), k(5), k(9)], 3).is_ok());
        assert!(check_key_lines(&[k(1), k(9)], 3).is_err(), "missing line");
        assert!(
            check_key_lines(&[k(5), k(1), k(9)], 3).is_err(),
            "out of order"
        );
        assert!(check_key_lines(&[k(1), ("tray_ready".into(), 0), k(2)], 2).is_ok());
    }

    // @R-006
    #[test]
    fn r006_keystroke_bar_is_p95_est_25_ms() {
        assert_eq!(decide(19.3), "PASS");
        assert_eq!(decide(25.0), "PASS");
        assert_eq!(decide(25.1), "FAIL");
    }

    // @R-006
    #[test]
    fn r006_keys_alternate_letter_and_backspace() {
        let v: Vec<u16> = (0..6).map(key).collect();
        assert_eq!(v, [0x58, 0x08, 0x43, 0x08, 0x58, 0x08]);
    }
}
