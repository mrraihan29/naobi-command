//! `baseline-powertoys --capture` (NCMD-6): the PowerToys Command Palette baseline, measured with the external
//! method of RK-15 and no in-process timestamps. t0 = QPC just before SendInput, t1 = first probe poll that sees
//! the change. The chord and the window rectangle are arguments, so nothing assumes which launcher is bound.
use crate::hotkey::{pinned_rate, run_once, PINNED_HZ};
use crate::percentile::percentile;
use anyhow::{anyhow, bail, Result};
use naobi_win32::inject;
use naobi_win32::platform::{qpc, qpc_to_ms};
use naobi_win32::probe::{self, Backdrop, Screen, BACKDROP};
use naobi_win32::process::ProcessMetrics;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Image-name prefixes of the PowerToys stack (PowerToys, Command Palette and its extension).
const PROCESS_PREFIXES: [&str; 2] = ["PowerToys", "Microsoft.CmdPal"];
/// Naobi's measured `inject_to_t1` p95 (RK-15 after tuning, docs/research/rk15-result.md).
const NAOBI_INJECT_TO_T1_P95: f64 = 28.76;
const IDLE_SECONDS: u64 = 60;
const MB: f64 = 1_000_000.0;
const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);
const KEY_TIMEOUT: Duration = Duration::from_secs(1);
/// Probe region as fractions of the window: the result list between the search row and the palette bottom,
/// inset from the sides so the app behind the palette does not leak into the hash.
const REGION: (f64, f64, f64, f64) = (0.05, 0.2, 0.9, 0.7);

/// The chords this capture can send. Command Palette on the owner's PC is bound to Left Ctrl+Space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Chord {
    CtrlSpace,
    AltSpace,
}

pub fn parse_chord(s: &str) -> Result<Chord> {
    match s.to_ascii_lowercase().as_str() {
        "ctrl+space" => Ok(Chord::CtrlSpace),
        "alt+space" => Ok(Chord::AltSpace),
        _ => bail!("unsupported chord {s}: use Ctrl+Space or Alt+Space"),
    }
}

fn fire(chord: Chord) {
    match chord {
        Chord::CtrlSpace => inject::ctrl_space(),
        Chord::AltSpace => inject::alt_space(),
    }
}

/// `x,y,w,h` in screen pixels.
pub fn parse_rect(s: &str) -> Result<(i32, i32, i32, i32)> {
    let v = s
        .split(',')
        .map(|p| p.trim().parse::<i32>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match v.as_slice() {
        &[x, y, w, h] if w > 0 && h > 0 => Ok((x, y, w, h)),
        _ => bail!("--rect wants x,y,w,h with w,h > 0"),
    }
}

/// `(image name, pid)` of the PowerToys stack, from `tasklist /FO CSV /NH` output.
pub fn parse_tasklist(csv: &str) -> Vec<(String, u32)> {
    csv.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split("\",\"").collect();
            let name = f.first()?.trim_matches('"');
            let pid = f.get(1)?.trim_matches('"').parse().ok()?;
            PROCESS_PREFIXES
                .iter()
                .any(|p| name.starts_with(p))
                .then(|| (name.to_string(), pid))
        })
        .collect()
}

/// Naobi against Command Palette on the same metric: `better`, `equal` or `worse` for Naobi (lower is better).
pub fn parity(naobi_p95: f64, cmdpal_p95: f64) -> &'static str {
    match naobi_p95.total_cmp(&cmdpal_p95) {
        std::cmp::Ordering::Less => "better",
        std::cmp::Ordering::Equal => "equal",
        std::cmp::Ordering::Greater => "worse",
    }
}

fn metric(name: &str, value: f64, unit: &str, n: usize, note: &str) -> Value {
    json!({ "name": name, "value": value, "unit": unit, "n": n, "note": note })
}

fn running_images() -> Result<Vec<(String, u32)>> {
    let out = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()?;
    Ok(parse_tasklist(&String::from_utf8_lossy(&out.stdout)))
}

fn image_running(image: &str) -> Result<bool> {
    let out = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {image}")])
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).contains(image))
}

/// Summed `(cpu_100ns, working set, private bytes)` over the pids.
fn sample_all(pids: &[u32]) -> Result<(u64, u64, u64)> {
    pids.iter().try_fold((0, 0, 0), |(c, w, p), &pid| {
        let m = ProcessMetrics::sample(pid)?;
        Ok((c + m.cpu_100ns, w + m.working_set, p + m.private_bytes))
    })
}

/// Size in bytes of every file under `dir`.
fn dir_size(dir: &Path) -> Result<u64> {
    let mut total = 0;
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        total += if e.file_type()?.is_dir() {
            dir_size(&e.path())?
        } else {
            e.metadata()?.len()
        };
    }
    Ok(total)
}

/// Busy percent of all logical CPUs between two `probe::system_cpu` readings.
fn busy_percent(from: (u64, u64), to: (u64, u64)) -> f64 {
    (to.0 - from.0) as f64 / (to.1 - from.1).max(1) as f64 * 100.0
}

/// Waits until `cond` holds, polling every 2 ms, for at most `SETTLE_TIMEOUT`.
fn settle(mut cond: impl FnMut() -> bool) -> Result<()> {
    let end = Instant::now() + SETTLE_TIMEOUT;
    while Instant::now() < end {
        if cond() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    bail!("the window did not settle within {SETTLE_TIMEOUT:?}")
}

/// Screen-space smoke check: opens Command Palette with the chord, saves a screenshot, prints the
/// foreground window (class, title, rectangle), then toggles it shut. The rectangle is the `--rect` input.
fn smoke(chord: Chord, shot: &Path) -> Result<()> {
    probe::dpi_aware();
    fire(chord);
    std::thread::sleep(Duration::from_secs(3));
    let script = format!(
        r#"Add-Type -AssemblyName System.Drawing,System.Windows.Forms
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class NW {{
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct RECT {{ public int L, T, R, B; }}
}}
"@
[void][NW]::SetProcessDPIAware()
$h = [NW]::GetForegroundWindow()
$r = New-Object NW+RECT
[void][NW]::GetWindowRect($h, [ref]$r)
$t = New-Object System.Text.StringBuilder 256; [void][NW]::GetWindowText($h, $t, 256)
$c = New-Object System.Text.StringBuilder 256; [void][NW]::GetClassName($h, $c, 256)
$s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $s.Width, $s.Height
[System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($s.Location, [System.Drawing.Point]::Empty, $s.Size)
$bmp.Save('{}')
"class={{0}} title={{1}} rect={{2}},{{3}},{{4}},{{5}}" -f $c, $t, $r.L, $r.T, ($r.R - $r.L), ($r.B - $r.T)"#,
        shot.display()
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;
    println!("{}", String::from_utf8_lossy(&out.stdout).trim());
    fire(chord);
    Ok(())
}

/// The capture: idle metrics with Command Palette hidden, then hotkey-to-visible and inject_to_t1 runs.
fn capture(
    chord: Chord,
    rect: (i32, i32, i32, i32),
    runs: usize,
    installs: &[PathBuf],
    out: &Path,
) -> Result<()> {
    if cfg!(debug_assertions) {
        bail!("ENV: a verdict needs a release build (cargo run --release)");
    }
    if !probe::on_ac_power() {
        bail!("ENV: not on AC power");
    }
    if image_running("naobi-app.exe")? {
        bail!("ENV: naobi-app.exe is running; quit it first");
    }
    let procs = running_images()?;
    if procs.is_empty() {
        bail!("ENV: no PowerToys process is running; start PowerToys with Command Palette");
    }
    let hz = probe::refresh_hz(rect);
    if !pinned_rate(hz) {
        bail!("ENV: display refresh is {hz} Hz, the verdict is pinned to {PINNED_HZ} Hz");
    }
    probe::dpi_aware();
    let backdrop = Backdrop::new(rect)?;
    backdrop.pump();
    std::thread::sleep(Duration::from_millis(300));

    let (x, y, w, h) = rect;
    let (px, py) = (x + w / 2, y + h / 20);
    let screen = Screen::new((8, 8));
    // Backdrop precondition: the contrasting pixel must show before the chord; otherwise Command Palette is open.
    if screen.pixel(px, py) != BACKDROP {
        bail!(
            "backdrop precondition failed: Command Palette is already visible at the probe point"
        );
    }
    let pids: Vec<u32> = procs.iter().map(|p| p.1).collect();

    // Idle: Command Palette hidden, PowerToys stack only.
    let sys0 = probe::system_cpu();
    let (c0, _, _) = sample_all(&pids)?;
    std::thread::sleep(Duration::from_secs(IDLE_SECONDS));
    let (c1, ws, private) = sample_all(&pids)?;
    let sys1 = probe::system_cpu();
    let idle_cpu = (c1 - c0) as f64 / 1e7 / IDLE_SECONDS as f64 * 100.0;

    // Hotkey-to-visible: t1 when the backdrop sample changes. `run_once` returns on pixel == bg, so the
    // probe maps "not the backdrop" to 0 and the backdrop to BACKDROP (bg = 0).
    let (mut hotkey_ms, mut hotkey_discarded) = (Vec::new(), 0);
    let mut attempts = 0;
    while hotkey_ms.len() < runs && attempts < runs * 2 {
        attempts += 1;
        let probe_px = || {
            if screen.pixel(px, py) == BACKDROP {
                BACKDROP
            } else {
                0
            }
        };
        match run_once(probe_px, 0, || fire(chord), qpc, Duration::from_secs(2)) {
            Some(ticks) => hotkey_ms.push(qpc_to_ms(ticks)),
            None => hotkey_discarded += 1,
        }
        if screen.pixel(px, py) != BACKDROP {
            fire(chord);
        }
        settle(|| screen.pixel(px, py) == BACKDROP)?;
        std::thread::sleep(Duration::from_millis(120));
    }
    if hotkey_ms.len() < runs {
        bail!(
            "only {} of {runs} valid hotkey runs ({hotkey_discarded} discarded)",
            hotkey_ms.len()
        );
    }

    // inject_to_t1: Command Palette visible, a key sent to it, t1 when the list region changes.
    fire(chord);
    settle(|| screen.pixel(px, py) != BACKDROP)?;
    std::thread::sleep(Duration::from_millis(500));
    let (fx, fy, fw, fh) = REGION;
    let (rx, ry) = (x + (fx * w as f64) as i32, y + (fy * h as f64) as i32);
    let mut region = Screen::new(((fw * w as f64) as i32, (fh * h as f64) as i32));
    let mut cost = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        region.region_hash(rx, ry);
        cost.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let probe_cost = percentile(&cost, 50.0).unwrap_or(f64::NAN);

    let (mut inject_ms, mut discarded) = (Vec::new(), 0);
    let mut attempts = 0;
    while inject_ms.len() < runs && attempts < runs * 2 {
        // Letter, Backspace, letter... so the list changes on every key.
        let vk: u16 = if attempts % 2 == 0 { 0x58 } else { 0x08 };
        attempts += 1;
        std::thread::sleep(Duration::from_millis(80));
        let pre = region.region_hash(rx, ry);
        let t_inject = qpc();
        probe::press(vk);
        let start = Instant::now();
        let t1 = loop {
            if region.region_hash(rx, ry) != pre {
                break Some(qpc());
            }
            if start.elapsed() > KEY_TIMEOUT {
                break None;
            }
        };
        match t1 {
            Some(t1) => inject_ms.push(qpc_to_ms(t1 - t_inject)),
            None => discarded += 1,
        }
    }
    fire(chord);
    settle(|| screen.pixel(px, py) == BACKDROP)?;
    if inject_ms.len() < runs {
        bail!(
            "only {} of {runs} valid inject runs ({discarded} discarded)",
            inject_ms.len()
        );
    }
    let sys2 = probe::system_cpu();

    let p = |v: &[f64], q: f64| percentile(v, q).unwrap_or(f64::NAN);
    let (hv50, hv95) = (p(&hotkey_ms, 50.0), p(&hotkey_ms, 95.0));
    let (i50, i95u, imax) = (
        p(&inject_ms, 50.0),
        p(&inject_ms, 95.0),
        p(&inject_ms, 100.0),
    );
    let i95e = i95u - probe_cost;
    let mut install_bytes = 0;
    for d in installs {
        install_bytes += dir_size(d)?;
    }
    let n_hv = hotkey_ms.len();
    let n_i = inject_ms.len();
    let mut metrics = vec![
        metric(
            "idle_working_set",
            ws as f64 / MB,
            "MB",
            1,
            "sum over the PowerToys processes at the end of 60 s idle",
        ),
        metric(
            "idle_private_bytes",
            private as f64 / MB,
            "MB",
            1,
            "sum of private bytes over the same processes",
        ),
        metric(
            "idle_cpu_percent",
            idle_cpu,
            "percent of one core",
            1,
            "sum over the PowerToys processes, 60 s idle, command palette hidden",
        ),
        metric(
            "idle_system_busy_percent",
            busy_percent(sys0, sys1),
            "percent",
            1,
            "all logical CPUs during the idle window (background load)",
        ),
        metric(
            "hotkey_to_visible_p50",
            hv50,
            "ms",
            n_hv,
            "chord SendInput to the first backdrop change",
        ),
        metric("hotkey_to_visible_p95", hv95, "ms", n_hv, "same runs"),
        metric(
            "inject_to_t1_p50",
            i50,
            "ms",
            n_i,
            "QPC before SendInput of the key to the probe t1",
        ),
        metric(
            "inject_to_t1_p95_upper",
            i95u,
            "ms",
            n_i,
            "conservative bound, as in rk15-result.md",
        ),
        metric(
            "inject_to_t1_p95_est",
            i95e,
            "ms",
            n_i,
            "p95_upper minus the probe cost; the true p95 lies between the two",
        ),
        metric("inject_to_t1_max", imax, "ms", n_i, "same runs"),
        metric(
            "probe_cost",
            probe_cost,
            "ms",
            200,
            "p50 of region hash on a static frame (one refresh period)",
        ),
        metric(
            "system_busy_percent_during_runs",
            busy_percent(sys1, sys2),
            "percent",
            1,
            "all logical CPUs during the latency runs",
        ),
    ];
    if installs.is_empty() {
        metrics.push(metric(
            "install_size",
            f64::NAN,
            "bytes",
            0,
            "not captured: no --install-dir given",
        ));
    } else {
        metrics.push(metric(
            "install_size",
            install_bytes as f64,
            "bytes",
            installs.len(),
            "sum over the --install-dir paths",
        ));
    }
    metrics.push(json!({ "name": "start_to_tray_ready", "value": null, "unit": "ms", "n": 0, "note": "not applicable: PowerToys is not started by the harness; it runs before the capture" }));

    let parity_result = parity(NAOBI_INJECT_TO_T1_P95, i95u);
    let doc = json!({
        "item": "NCMD-6",
        "captured_at_unix": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "subject": "PowerToys Command Palette",
        "chord": format!("{chord:?}"),
        "rect": [x, y, w, h],
        "machine": std::env::var("COMPUTERNAME").unwrap_or_default(),
        "refresh_hz": hz,
        "pinned_hz": PINNED_HZ,
        "ac_power": true,
        "release_build": true,
        "processes": procs.iter().map(|(n, p)| json!({ "name": n, "pid": p })).collect::<Vec<_>>(),
        "discarded": { "hotkey": hotkey_discarded, "inject": discarded },
        "metrics": metrics,
        "naobi_vs_cmdpal_inject_to_t1": {
            "naobi_p95_ms": NAOBI_INJECT_TO_T1_P95,
            "cmdpal_p95_upper_ms": i95u,
            "naobi_is": parity_result,
        },
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(&doc)?)?;
    println!("{doc}");
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let mut it = args.iter();
    let (mut capture_on, mut smoke_on) = (false, false);
    let mut hotkey = "Ctrl+Space".to_string();
    let mut rect = None;
    let mut runs = 200usize;
    let mut out = PathBuf::from("docs/research/baseline-powertoys.json");
    let mut shot = PathBuf::from("baseline-smoke.png");
    let mut installs = Vec::new();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| anyhow!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--capture" => capture_on = true,
            "--smoke" => smoke_on = true,
            "--hotkey" => hotkey = value()?,
            "--rect" => rect = Some(value()?),
            "--runs" => runs = value()?.parse()?,
            "--out" => out = PathBuf::from(value()?),
            "--shot" => shot = PathBuf::from(value()?),
            "--install-dir" => installs.push(PathBuf::from(value()?)),
            _ => bail!("unknown flag {flag}"),
        }
    }
    let chord = parse_chord(&hotkey)?;
    if smoke_on {
        return smoke(chord, &shot);
    }
    if !capture_on {
        bail!("usage: naobi-perf baseline-powertoys --capture --rect x,y,w,h [--hotkey Ctrl+Space] [--runs n] [--out f] [--install-dir d]... | --smoke [--shot png]");
    }
    let rect = parse_rect(&rect.ok_or_else(|| anyhow!("--rect x,y,w,h is required"))?)?;
    capture(chord, rect, runs, &installs, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_rect_and_tasklist_parse() {
        assert_eq!(parse_chord("Ctrl+Space").unwrap(), Chord::CtrlSpace);
        assert_eq!(parse_chord("alt+space").unwrap(), Chord::AltSpace);
        assert!(parse_chord("Win+Alt+Space").is_err());
        assert_eq!(parse_rect("10,20,300,400").unwrap(), (10, 20, 300, 400));
        assert!(parse_rect("10,20,0,400").is_err());
        assert!(parse_rect("10,20,300").is_err());
        let csv = "\"PowerToys.exe\",\"27376\",\"Console\",\"5\",\"116,220 K\"\n\"Microsoft.CmdPal.UI.exe\",\"34312\",\"Console\",\"5\",\"228,588 K\"\n\"explorer.exe\",\"100\",\"Console\",\"5\",\"1 K\"";
        assert_eq!(
            parse_tasklist(csv),
            vec![
                ("PowerToys.exe".to_string(), 27376),
                ("Microsoft.CmdPal.UI.exe".to_string(), 34312)
            ]
        );
    }

    #[test]
    fn parity_is_lower_is_better() {
        assert_eq!(parity(28.76, 40.0), "better");
        assert_eq!(parity(40.0, 28.76), "worse");
        assert_eq!(parity(28.76, 28.76), "equal");
    }
}
