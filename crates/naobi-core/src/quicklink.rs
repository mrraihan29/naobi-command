//! Quicklinks: URL templates with a `{query}` placeholder, opened in the default browser.
use serde::Deserialize;

const PLACEHOLDER: &str = "{query}";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quicklink {
    pub name: String,
    pub alias: String,
    pub url: String,
}

/// `(scheme, authority)` of an http(s) URL, parsed by hand (no URL crate).
fn scheme_authority(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let auth = &rest[..end];
    // A host is required and userinfo is refused.
    (!auth.is_empty() && !auth.contains('@')).then_some((scheme, auth))
}

impl Quicklink {
    /// T-004: http/https only, `{query}` present and never inside the host.
    pub fn validate(&self) -> Result<(), String> {
        if self.alias.trim().is_empty() || self.name.trim().is_empty() {
            return Err("quicklink needs a name and an alias".into());
        }
        if self.alias.contains(char::is_whitespace) {
            return Err("quicklink alias cannot contain whitespace".into());
        }
        let (_, auth) =
            scheme_authority(&self.url).ok_or("quicklink url must be http or https with a host")?;
        if auth.contains(PLACEHOLDER) {
            return Err("{query} is not allowed in the host".into());
        }
        if !self.url.contains(PLACEHOLDER) {
            return Err("quicklink url needs a {query} placeholder".into());
        }
        Ok(())
    }

    /// T-006: percent-encodes `query` (RFC 3986 unreserved set stays) and re-checks the result.
    pub fn build_url(&self, query: &str) -> Result<String, String> {
        self.validate()?;
        let url = self.url.replace(PLACEHOLDER, &percent_encode(query));
        match (scheme_authority(&self.url), scheme_authority(&url)) {
            (Some(before), Some(after)) if before == after => Ok(url),
            _ => Err("built url changed scheme or host".into()),
        }
    }
}

/// Percent-encodes `s` keeping the RFC 3986 unreserved set.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
