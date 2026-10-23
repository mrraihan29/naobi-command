use naobi_core::quicklink::Quicklink;
use naobi_core::settings::Settings;

fn ql(url: &str) -> Quicklink {
    Quicklink {
        name: "T".into(),
        alias: "t".into(),
        url: url.into(),
    }
}

// @R-021
#[test]
fn r021_non_http_scheme_rejected() {
    let text = r#"
[[quicklinks]]
name = "Bad"
alias = "b"
url = "ftp://example.com/?q={query}"

[[quicklinks]]
name = "Js"
alias = "j"
url = "javascript:alert({query})"

[[quicklinks]]
name = "NoPlaceholder"
alias = "n"
url = "https://example.com/"

[[quicklinks]]
name = "HostHole"
alias = "h"
url = "https://{query}.example.com/"

[[quicklinks]]
name = "Ok"
alias = "ok"
url = "https://example.com/?q={query}"
"#;
    let p = Settings::parse(text).unwrap();
    let aliases: Vec<_> = p
        .settings
        .quicklinks
        .iter()
        .map(|q| q.alias.as_str())
        .collect();
    assert_eq!(aliases, ["ok"]);
    assert_eq!(p.reports.len(), 4);

    let dup = r#"
[[quicklinks]]
name = "A"
alias = "a"
url = "https://e.com/{query}"
[[quicklinks]]
name = "B"
alias = "a"
url = "https://f.com/{query}"
[[quicklinks]]
name = "C"
alias = "c d"
url = "https://g.com/{query}"
"#;
    let p = Settings::parse(dup).unwrap();
    assert_eq!(p.settings.quicklinks.len(), 1);
    assert_eq!(p.settings.quicklinks[0].name, "A");
    assert_eq!(p.reports.len(), 2);
}

// @R-021
#[test]
fn r021_query_is_percent_encoded() {
    let q = ql("https://example.com/s?q={query}");
    assert_eq!(
        q.build_url("a&b #1 %").unwrap(),
        "https://example.com/s?q=a%26b%20%231%20%25"
    );
    assert_eq!(
        q.build_url("A-z_0.9~").unwrap(),
        "https://example.com/s?q=A-z_0.9~"
    );
    // The host cannot be changed by the query.
    let url = q.build_url("@evil.com/\\").unwrap();
    assert!(url.starts_with("https://example.com/s?q="));
    assert!(!url.contains('@') && !url.contains('\\'));
}
