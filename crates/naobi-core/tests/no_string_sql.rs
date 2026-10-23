//! T-007: SQL is never built from strings. Scans workspace sources; tier 1.
use std::path::{Path, PathBuf};

const KEYWORDS: [&str; 8] = [
    "SELECT ", "INSERT ", "UPDATE ", "DELETE ", "DROP ", "CREATE ", "ALTER ", "PRAGMA ",
];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// A line is suspect when it holds an SQL keyword literal and is (or sits right under) a
/// `format!`, `push_str` or `+` string join.
fn string_built_sql(lines: &[&str], i: usize) -> bool {
    let line = lines[i];
    if !KEYWORDS.iter().any(|k| line.contains(k)) {
        return false;
    }
    let opens = |l: &str| l.contains("format!(") || l.contains("push_str(");
    let prev_opens = i > 0 && {
        let p = lines[i - 1].trim_end();
        p.ends_with("format!(") || p.ends_with("push_str(")
    };
    opens(line)
        || prev_opens
        || line.contains("\" +")
        || line.contains("+ \"")
        || line.contains("+ &")
}

fn scan(src: &str) -> Vec<usize> {
    let lines: Vec<&str> = src.lines().collect();
    (0..lines.len())
        .filter(|&i| string_built_sql(&lines, i))
        .collect()
}

#[test]
fn d014_no_string_built_sql() {
    // The detector itself must catch the usual shapes.
    assert!(!scan("let q = format!(\"SELECT * FROM t WHERE a = {}\", a);").is_empty());
    assert!(!scan("let q = \"DELETE FROM t WHERE a = \".to_string() + &a;").is_empty());
    assert!(scan("conn.execute(\"DELETE FROM t WHERE a = ?1\", [a]);").is_empty());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    let me = Path::new(file!()).file_name().unwrap().to_owned();
    let mut bad = Vec::new();
    for f in files.iter().filter(|f| f.file_name() != Some(&me)) {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        for i in scan(&src) {
            bad.push(format!("{}:{}", f.display(), i + 1));
        }
    }
    assert!(bad.is_empty(), "SQL built from strings: {bad:?}");
}
