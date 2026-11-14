//! `all --out target/perf.json` (R-006, R-002): every R-006 budget on the finished product in one run, one
//! verdict file, a table, and a non-zero exit on any FAIL. The latency runs refuse to start unless the
//! pinned conditions hold (`hotkey::check_env`, then the panel rate inside `prepare`), and a verdict whose
//! own `conditions_valid` is false is a FAIL. The keystroke bar is the live `p95_est <= 25 ms` of D-037; no
//! result is accepted from a D- entry because none is needed. Idle is taken in the heaviest state (one pinned
//! window, so a border and a hook exist, and a timed Awake, so a power request and two timers exist).
use crate::verdict::{record, verdict};
use crate::{cold_start, exe_size, hotkey, idle, keystroke, network_silence};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::Path;

/// The committed PowerToys Command Palette baseline (NCMD-6).
const BASELINE: &str = "docs/research/baseline-powertoys.json";
const ITEMS: usize = 10_000;
/// Every budget `all` must end with, in the order of the table.
const EXPECTED: [&str; 9] = [
    "hotkey_to_visible_p95",
    "keystroke_to_results_p95_est",
    "keystroke_parity_with_command_palette",
    "start_to_tray_ready",
    "start_to_window_ready",
    "idle_working_set",
    "idle_cpu",
    "release_exe_size",
    "r010_no_socket_during_session",
];

/// `inject_to_t1` p95 of Command Palette, the like-for-like column for Naobi's `inject_to_t1_ms.p95`.
fn baseline_p95(doc: &Value) -> Option<f64> {
    doc["metrics"]
        .as_array()?
        .iter()
        .find(|m| m["name"] == "inject_to_t1_p95_upper")?["value"]
        .as_f64()
}

/// Naobi's keystroke `inject_to_t1` p95 against the baseline: no worse than Command Palette (R-006 v5).
fn parity(entries: &[Value], baseline: f64) -> Result<Value> {
    let naobi = entries
        .iter()
        .find(|e| e["budget"] == "keystroke_to_results_p95_est")
        .and_then(|e| e["inject_to_t1_ms"]["p95"].as_f64())
        .ok_or_else(|| anyhow!("no keystroke result to compare"))?;
    let mut v = verdict(
        "keystroke_parity_with_command_palette",
        naobi,
        baseline,
        "ms",
    );
    v["column"] = json!("inject_to_t1 p95 (Command Palette: inject_to_t1_p95_upper)");
    v["baseline_file"] = json!(BASELINE);
    Ok(v)
}

/// Checks the finished file: every expected budget is there, passes, and was taken under valid conditions.
/// Returns the table lines; the error names every failure.
fn judge(entries: &[Value]) -> Result<Vec<String>> {
    let (mut table, mut bad) = (Vec::new(), Vec::new());
    for name in EXPECTED {
        match entries.iter().find(|e| e["budget"] == name) {
            None => bad.push(format!("{name}: no result")),
            Some(e) => {
                let ok = e["pass"] == true && e["conditions_valid"] != false;
                if !ok {
                    bad.push(format!("{name}: {}", e["value"]));
                }
                table.push(format!(
                    "{}  {name}: {} (limit {} {})",
                    if ok { "PASS" } else { "FAIL" },
                    e["value"],
                    e["limit"],
                    e["unit"].as_str().unwrap_or("")
                ));
            }
        }
    }
    if bad.is_empty() {
        Ok(table)
    } else {
        for l in &table {
            println!("{l}");
        }
        bail!("FAIL: {}", bad.join("; "))
    }
}

pub fn run(exe: &Path, out: &Path, idle_seconds: u64, minutes: u64) -> Result<()> {
    // Fail at once, before any minute is spent, when the conditions cannot hold.
    hotkey::check_env()?;
    let baseline: Value = serde_json::from_str(&std::fs::read_to_string(BASELINE)?)?;
    let baseline =
        baseline_p95(&baseline).ok_or_else(|| anyhow!("{BASELINE} has no inject_to_t1"))?;
    match std::fs::remove_file(out) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    // The panel rate is checked first, inside `prepare`, so a wrong display ends the run in seconds.
    hotkey::run(exe, 200, out)?;
    keystroke::run(exe, 200, ITEMS, out)?;
    cold_start::run(exe, 10, out)?;
    idle::run(exe, idle_seconds, out, true, true)?;
    exe_size::run(exe, out)?;
    network_silence::run(exe, minutes, out)?;
    let entries: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(out)?)?;
    record(out, parity(&entries, baseline)?)?;
    let entries: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(out)?)?;
    for line in judge(&entries)? {
        println!("{line}");
    }
    println!("all R-006 budgets PASS; verdicts in {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, pass: bool) -> Value {
        json!({"budget": name, "value": 1.0, "limit": 2.0, "unit": "u", "pass": pass})
    }

    fn all_pass() -> Vec<Value> {
        EXPECTED.iter().map(|n| entry(n, true)).collect()
    }

    // @R-006
    #[test]
    fn r006_all_passes_only_when_every_budget_is_there_and_passes() {
        assert!(judge(&all_pass()).is_ok());
        let mut failed = all_pass();
        failed[3] = entry(EXPECTED[3], false);
        assert!(judge(&failed).is_err(), "a failing budget");
        let mut missing = all_pass();
        missing.pop();
        assert!(judge(&missing).is_err(), "a missing budget");
        let mut unpinned = all_pass();
        unpinned[0]["conditions_valid"] = json!(false);
        assert!(
            judge(&unpinned).is_err(),
            "conditions that were not the pinned ones"
        );
    }

    // @R-006
    #[test]
    fn r006_parity_reads_the_committed_baseline_like_for_like() {
        let doc: Value = serde_json::from_str(include_str!(
            "../../../docs/research/baseline-powertoys.json"
        ))
        .unwrap_or_default();
        let base = baseline_p95(&doc).unwrap_or(f64::NAN);
        assert!(
            base > 100.0,
            "Command Palette inject_to_t1 is in the hundreds of ms: {base}"
        );
        let mut k = entry("keystroke_to_results_p95_est", true);
        k["inject_to_t1_ms"] = json!({"p95": 28.76});
        assert_eq!(
            parity(&[k.clone()], base).map(|v| v["pass"] == true).ok(),
            Some(true)
        );
        k["inject_to_t1_ms"] = json!({"p95": base + 1.0});
        assert_eq!(
            parity(&[k], base).map(|v| v["pass"] == true).ok(),
            Some(false)
        );
        assert!(parity(&[], base).is_err());
    }
}
