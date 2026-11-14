mod baseline;
mod cold_start;
mod exe_size;
mod hotkey;
mod idle;
mod keystroke;
mod percentile;
mod verdict;

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

const USAGE: &str = "usage: naobi-perf idle --exe <path> --seconds <n> [--pinned 1] [--awake 1] [--out <file>]\n       naobi-perf exe-size --exe <path> [--out <file>]\n       naobi-perf hotkey-to-visible [--exe <path>] [--runs <n>]\n       naobi-perf keystroke-to-results [--exe <path>] [--runs <n>] [--items <n>]\n       naobi-perf cold-start [--exe <path>] [--runs <n>]\n(the last three default to target/release/naobi-app.exe, built with --features perf)";

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().ok_or_else(|| anyhow!(USAGE))?;
    if cmd == "baseline-powertoys" {
        return baseline::run(&args.collect::<Vec<_>>());
    }
    let (mut exe, mut seconds, mut out) = (None, 60, PathBuf::from("target/perf.json"));
    let (mut runs, mut items) = (None::<usize>, 10_000usize);
    let mut pinned = false;
    let mut awake = false;
    while let Some(flag) = args.next() {
        let val = args.next().ok_or_else(|| anyhow!("{flag} needs a value"))?;
        match flag.as_str() {
            "--exe" => exe = Some(PathBuf::from(val)),
            "--seconds" => seconds = val.parse()?,
            "--runs" => runs = Some(val.parse()?),
            "--items" => items = val.parse()?,
            "--out" => out = PathBuf::from(val),
            "--pinned" => pinned = val != "0",
            "--awake" => awake = val != "0",
            _ => bail!("unknown flag {flag}\n{USAGE}"),
        }
    }
    let default_exe = exe
        .clone()
        .unwrap_or_else(|| PathBuf::from("target/release/naobi-app.exe"));
    let exe = exe.ok_or_else(|| anyhow!("--exe is required\n{USAGE}"));
    match cmd.as_str() {
        "idle" => idle::run(&exe?, seconds, &out, pinned, awake),
        "exe-size" => exe_size::run(&exe?, &out),
        "hotkey-to-visible" => hotkey::run(&default_exe, runs.unwrap_or(200), &out),
        "keystroke-to-results" => keystroke::run(&default_exe, runs.unwrap_or(200), items, &out),
        "cold-start" => cold_start::run(&default_exe, runs.unwrap_or(10), &out),
        _ => bail!("{USAGE}"),
    }
}
