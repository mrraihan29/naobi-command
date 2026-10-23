use crate::verdict::{record, verdict};
use anyhow::Result;
use std::path::Path;

const LIMIT_BYTES: f64 = 20_000_000.0;

/// Reports the exe size in bytes with PASS or FAIL against 20 MB (R-006).
pub fn run(exe: &Path, out: &Path) -> Result<()> {
    let bytes = std::fs::metadata(exe)?.len();
    let v = verdict("release_exe_size", bytes as f64, LIMIT_BYTES, "bytes");
    println!("{} {v}", if v["pass"] == true { "PASS" } else { "FAIL" });
    record(out, v)
}
