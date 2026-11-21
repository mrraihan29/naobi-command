use naobi_core::settings::MAX_FILE_BYTES;
use naobi_core::snippet::{append_snippet, Snippet, SnippetsError, SnippetsFile};
use std::fs;
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("naobi-snippet-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        if let Err(e) = fs::create_dir_all(&p) {
            panic!("temp dir: {e}");
        }
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parse_valid_empty_and_populated_snippets_toml() {
    let empty = SnippetsFile::parse("").expect("empty parses");
    assert!(empty.snippets.is_empty());
    assert!(empty.reports.is_empty());

    let toml = r#"
# Owner's snippets
[[snippets]]
name = "Email signature"
alias = "sig"
text = "Best regards,\nRaihan"

[[snippets]]
name = "Standup"
text = "Date: {date}\nTime: {time}\nID: {uuid}\nLink: {clipboard}"
"#;
    let parsed = SnippetsFile::parse(toml).expect("valid toml");
    assert!(parsed.reports.is_empty(), "{:?}", parsed.reports);
    assert_eq!(
        parsed.snippets,
        vec![
            Snippet {
                name: "Email signature".into(),
                alias: Some("sig".into()),
                text: "Best regards,\nRaihan".into(),
            },
            Snippet {
                name: "Standup".into(),
                alias: None,
                text: "Date: {date}\nTime: {time}\nID: {uuid}\nLink: {clipboard}".into(),
            },
        ]
    );
}

#[test]
fn invalid_toml_and_oversize_keep_last_valid_and_report_error() {
    let prev = vec![Snippet {
        name: "Greeting".into(),
        alias: Some("hi".into()),
        text: "Hello!".into(),
    }];

    // Syntax error on line 3.
    let bad_syntax = "[[snippets]]\nname = \"x\"\nbroken = = 1\n";
    assert!(matches!(
        SnippetsFile::parse(bad_syntax),
        Err(SnippetsError::Syntax { line: Some(3), .. })
    ));
    let fallback = SnippetsFile::apply(&prev, bad_syntax);
    assert_eq!(fallback.snippets, prev);
    assert_eq!(fallback.reports.len(), 1);
    assert_eq!(fallback.reports[0].line, Some(3));

    // Unknown field is strictly rejected.
    let unknown_key = "[[snippets]]\nname = \"x\"\ntxt = \"y\"\n";
    assert!(SnippetsFile::parse(unknown_key).is_err());
    assert_eq!(SnippetsFile::apply(&prev, unknown_key).snippets, prev);

    // Oversize file is rejected.
    let too_big = format!("#{}\n", "x".repeat(MAX_FILE_BYTES));
    assert_eq!(SnippetsFile::parse(&too_big), Err(SnippetsError::TooLarge));
    let big_fallback = SnippetsFile::apply(&prev, &too_big);
    assert_eq!(big_fallback.snippets, prev);
    assert_eq!(big_fallback.reports.len(), 1);
}

#[test]
fn invalid_entries_and_duplicate_aliases_are_reported_while_valid_entries_stay() {
    let toml = r#"
[[snippets]]
name = ""
text = "missing name"

[[snippets]]
name = "Empty text"
text = ""

[[snippets]]
name = "Bad alias"
alias = "two words"
text = "nope"

[[snippets]]
name = "First"
alias = "dup"
text = "one"

[[snippets]]
name = "Second"
alias = "DUP"
text = "two"
"#;
    let parsed = SnippetsFile::parse(toml).expect("valid TOML syntax");
    assert_eq!(
        parsed.snippets,
        vec![
            Snippet {
                name: "First".into(),
                alias: Some("dup".into()),
                text: "one".into(),
            },
            Snippet {
                name: "Second".into(),
                alias: None,
                text: "two".into(),
            },
        ]
    );
    assert_eq!(parsed.reports.len(), 4, "{:?}", parsed.reports);
}

#[test]
fn append_snippet_creates_preserves_comments_deduplicates_names_and_refuses_broken_file() {
    let dir = Dir::new("append");
    let path = dir.0.join("snippets.toml");

    // Creates file on first save.
    let first = append_snippet(&path, "Hello \"world\"\nLine 2").expect("first append");
    assert_eq!(first.name, "Hello \"world\"");
    assert_eq!(first.text, "Hello \"world\"\nLine 2");

    // Preserves existing owner comments and deduplicates identical first-line names.
    let original = fs::read_to_string(&path).expect("read");
    let with_comment = format!("# Keep my comment\n{original}");
    fs::write(&path, &with_comment).expect("write");

    let second = append_snippet(&path, "Hello \"world\"\nLine 2").expect("second append");
    assert_eq!(second.name, "Hello \"world\" (2)");

    let updated = fs::read_to_string(&path).expect("read updated");
    assert!(updated.contains("# Keep my comment"));
    let parsed = SnippetsFile::parse(&updated).expect("re-parses cleanly");
    assert_eq!(parsed.snippets.len(), 2);
    assert_eq!(parsed.snippets[0].text, "Hello \"world\"\nLine 2");
    assert_eq!(parsed.snippets[1].name, "Hello \"world\" (2)");

    // Empty text is rejected.
    assert!(append_snippet(&path, "").is_err());

    // Broken TOML on disk is left untouched and never overwritten.
    fs::write(&path, "[[snippets]\nbroken").expect("break");
    assert!(append_snippet(&path, "new text").is_err());
    assert_eq!(
        fs::read_to_string(&path).expect("kept"),
        "[[snippets]\nbroken"
    );
}
