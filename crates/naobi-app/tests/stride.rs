//! R-010: every threat in the STRIDE table of REPORT 6.2 ends in a mitigation that is checked: a test that
//! exists in the workspace, a verdict or command that exists, an accepted-risk D- entry that exists in the
//! ledger, or a stated reason why it cannot be tested. A planned test name that was never written, or was
//! renamed, fails here.
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "target") {
                rust_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The last cell of each `| T-nnn |` row of REPORT 6.2, with its threat id.
fn threats(report: &str) -> Vec<(String, String)> {
    report
        .lines()
        .filter(|l| l.starts_with("| T-"))
        .filter_map(|l| {
            let cells: Vec<&str> = l.trim().trim_matches('|').split(" | ").collect();
            // Seven columns: id, STRIDE, element, threat, mitigation, ASVS, test or risk.
            if cells.len() != 7 {
                return None;
            }
            Some((
                cells.first()?.trim().to_owned(),
                cells.last()?.trim().to_owned(),
            ))
        })
        .collect()
}

/// The `` `backticked` `` words of a cell.
fn ticks(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// `r012_name` or `d014_name`: how a test (or a verdict id) is named.
fn is_test_name(word: &str) -> bool {
    let mut parts = word.splitn(2, '_');
    let (head, tail) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    head.len() == 4
        && matches!(head.as_bytes()[0], b'r' | b'd')
        && head[1..].bytes().all(|b| b.is_ascii_digit())
        && !tail.is_empty()
        && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Accepted-risk ids named in a cell, `D-026` and the like.
fn risk_ids(cell: &str) -> Vec<&str> {
    cell.match_indices("D-")
        .filter_map(|(i, _)| cell.get(i..i + 5))
        .filter(|id| id[2..].bytes().all(|b| b.is_ascii_digit()))
        .collect()
}

// @R-010
#[test]
fn r010_stride_rows_have_mitigation_test_or_risk() {
    let report = std::fs::read_to_string(root().join("docs/design/REPORT.md"))
        .unwrap_or_else(|e| panic!("REPORT.md: {e}"));
    let ledger = std::fs::read_to_string(root().join("docs/ledger.yaml"))
        .unwrap_or_else(|e| panic!("ledger.yaml: {e}"));
    let perf = std::fs::read_to_string(root().join("crates/naobi-perf/src/main.rs"))
        .unwrap_or_else(|e| panic!("naobi-perf main.rs: {e}"));
    let mut files = Vec::new();
    rust_files(&root().join("crates"), &mut files);
    let sources: Vec<String> = files
        .iter()
        .filter(|p| !p.ends_with("tests/stride.rs"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    let exists = |name: &str| {
        let (f, s) = (format!("fn {name}("), format!("\"{name}\""));
        sources.iter().any(|t| t.contains(&f) || t.contains(&s))
    };

    let rows = threats(&report);
    // The report states its own total; a table that stops parsing must not pass by being empty.
    let total: usize = report
        .lines()
        .find(|l| l.starts_with("Threat counts:"))
        .and_then(|l| l.split("total ").nth(1))
        .and_then(|t| t.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(usize::MAX);
    assert_eq!(rows.len(), total, "rows parsed against the stated total");

    let mut bad = Vec::new();
    for (id, cell) in &rows {
        let names: Vec<&str> = ticks(cell)
            .into_iter()
            .filter(|w| is_test_name(w))
            .collect();
        let missing: Vec<&&str> = names.iter().filter(|n| !exists(n)).collect();
        if !missing.is_empty() {
            bad.push(format!("{id}: no such test {missing:?}"));
            continue;
        }
        let risks = risk_ids(cell);
        if let Some(r) = risks.iter().find(|r| !ledger.contains(&format!("id: {r}"))) {
            bad.push(format!("{id}: {r} is not in the ledger"));
            continue;
        }
        let command = ticks(cell).iter().any(|w| {
            *w == "cargo deny check"
                || w.strip_prefix("naobi-perf ")
                    .and_then(|c| c.split(' ').next())
                    .is_some_and(|c| perf.contains(&format!("naobi-perf {c}")))
        });
        let stated = cell.contains("Not unit-testable");
        if names.is_empty() && risks.is_empty() && !command && !stated {
            bad.push(format!("{id}: no test, command, accepted risk or reason"));
        }
    }
    assert!(
        bad.is_empty(),
        "STRIDE rows without a checked mitigation:\n{}",
        bad.join("\n")
    );
}

// @R-010
#[test]
fn r010_the_row_reader_tells_names_risks_and_commands_apart() {
    let rows = threats(
        "| T-001 | S | E | T | M | V16 | `r008_a_b`, residual D-029 |\n| T-002 | x |\nnot a row",
    );
    assert_eq!(
        rows,
        [("T-001".to_owned(), "`r008_a_b`, residual D-029".to_owned())]
    );
    assert_eq!(
        ticks("`r008_a_b`, and `cargo deny check` bans"),
        ["r008_a_b", "cargo deny check"]
    );
    assert!(is_test_name("r008_conflict_notice") && is_test_name("d014_x"));
    assert!(
        !is_test_name("cargo deny check") && !is_test_name("naobi_perf") && !is_test_name("r8_x")
    );
    assert_eq!(
        risk_ids("Accepted risk D-026 (residual) and D-029"),
        ["D-026", "D-029"]
    );
}
