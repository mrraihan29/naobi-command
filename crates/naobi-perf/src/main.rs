mod exe_size;
mod idle;
#[allow(dead_code)] // reason: used by the hotkey and keystroke subcommands of later items
mod percentile;
mod verdict;

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

const USAGE: &str = "usage: naobi-perf idle --exe <path> --seconds <n> [--out <file>]\n       naobi-perf exe-size --exe <path> [--out <file>]";

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().ok_or_else(|| anyhow!(USAGE))?;
    let (mut exe, mut seconds, mut out) = (None, 60, PathBuf::from("target/perf.json"));
    while let Some(flag) = args.next() {
        let val = args.next().ok_or_else(|| anyhow!("{flag} needs a value"))?;
        match flag.as_str() {
            "--exe" => exe = Some(PathBuf::from(val)),
            "--seconds" => seconds = val.parse()?,
            "--out" => out = PathBuf::from(val),
            _ => bail!("unknown flag {flag}\n{USAGE}"),
        }
    }
    let exe = exe.ok_or_else(|| anyhow!("--exe is required\n{USAGE}"));
    match cmd.as_str() {
        "idle" => idle::run(&exe?, seconds, &out),
        "exe-size" => exe_size::run(&exe?, &out),
        _ => bail!("{USAGE}"),
    }
}
