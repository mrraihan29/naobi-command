use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;

/// One budget result: `{budget, value, limit, unit, pass}`. Passes when `value <= limit`.
pub fn verdict(budget: &str, value: f64, limit: f64, unit: &str) -> Value {
    json!({ "budget": budget, "value": value, "limit": limit, "unit": unit, "pass": value <= limit })
}

/// Adds the entry to the JSON array in `out` (created if missing), replacing the same budget.
pub fn record(out: &Path, entry: Value) -> Result<()> {
    let mut all: Vec<Value> = match std::fs::read_to_string(out) {
        Ok(s) => serde_json::from_str(&s)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    all.retain(|e| e["budget"] != entry["budget"]);
    all.push(entry);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(&all)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_shape_and_pass() {
        let v = verdict("exe_size", 10.0, 20.0, "MB");
        assert_eq!(
            v,
            json!({"budget": "exe_size", "value": 10.0, "limit": 20.0, "unit": "MB", "pass": true})
        );
        assert_eq!(verdict("x", 21.0, 20.0, "MB")["pass"], false);
    }

    #[test]
    fn record_appends_and_replaces_same_budget() {
        let out = std::env::temp_dir().join(format!("naobi-perf-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&out);
        record(&out, verdict("a", 1.0, 2.0, "u")).unwrap();
        record(&out, verdict("b", 1.0, 2.0, "u")).unwrap();
        record(&out, verdict("a", 3.0, 2.0, "u")).unwrap();
        let all: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["pass"], false);
    }
}
