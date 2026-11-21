//! Snippets model, strict `snippets.toml` parser, and atomic append helper.
//! Never logs snippet names, aliases, or text (D-020).
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::settings::{Report, MAX_FILE_BYTES};

pub const FILE_NAME: &str = "snippets.toml";
const TITLE_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnippetsError {
    #[error("snippets file is larger than 256 KiB")]
    TooLarge,
    #[error("{message}")]
    Syntax {
        line: Option<usize>,
        message: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedSnippets {
    pub snippets: Vec<Snippet>,
    pub reports: Vec<Report>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct RawFile {
    snippets: Vec<Snippet>,
}

pub struct SnippetsFile;

impl SnippetsFile {
    /// Strict TOML parse. Syntax errors, unknown keys, and oversize files fail the whole file;
    /// invalid entries or duplicate aliases are reported in `reports`.
    pub fn parse(text: &str) -> Result<ParsedSnippets, SnippetsError> {
        if text.len() > MAX_FILE_BYTES {
            return Err(SnippetsError::TooLarge);
        }
        let raw: RawFile = toml::from_str(text).map_err(|e| SnippetsError::Syntax {
            line: e.span().and_then(|s| {
                let head = text.as_bytes().get(..s.start)?;
                Some(head.iter().filter(|&&b| b == b'\n').count() + 1)
            }),
            message: e.message().to_string(),
        })?;

        let mut snippets: Vec<Snippet> = Vec::new();
        let mut reports = Vec::new();
        let mut note = |message: String| {
            reports.push(Report {
                line: None,
                message,
            });
        };

        for mut entry in raw.snippets {
            let trimmed_name = entry.name.trim();
            if trimmed_name.is_empty() {
                note("snippet rejected: name cannot be empty".into());
                continue;
            }
            if entry.text.is_empty() {
                note(format!(
                    "snippet '{trimmed_name}' rejected: text cannot be empty"
                ));
                continue;
            }
            entry.name = trimmed_name.to_owned();

            let normalized_alias = match entry.alias.take() {
                Some(a) => {
                    let t = a.trim();
                    if t.is_empty() {
                        None
                    } else if t.contains(char::is_whitespace) {
                        note(format!(
                            "snippet '{}' rejected: alias cannot contain whitespace",
                            entry.name
                        ));
                        continue;
                    } else if snippets.iter().any(|s| {
                        s.alias
                            .as_deref()
                            .is_some_and(|o| o.eq_ignore_ascii_case(t))
                    }) {
                        note(format!(
                            "snippet '{}' alias '{t}' ignored: duplicate alias",
                            entry.name
                        ));
                        None
                    } else {
                        Some(t.to_owned())
                    }
                }
                None => None,
            };
            entry.alias = normalized_alias;
            snippets.push(entry);
        }

        Ok(ParsedSnippets { snippets, reports })
    }

    /// D-009: an invalid file keeps `prev` and reports why.
    pub fn apply(prev: &[Snippet], text: &str) -> ParsedSnippets {
        Self::parse(text).unwrap_or_else(|e| ParsedSnippets {
            snippets: prev.to_vec(),
            reports: vec![Report {
                line: match &e {
                    SnippetsError::Syntax { line, .. } => *line,
                    SnippetsError::TooLarge => None,
                },
                message: e.to_string(),
            }],
        })
    }
}

/// Derives a non-empty single-line snippet name from `text`, disambiguated against `existing`.
fn derive_name(text: &str, existing: &[Snippet]) -> String {
    let base: String = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Snippet")
        .chars()
        .take(TITLE_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let base = if base.trim().is_empty() {
        "Snippet".to_owned()
    } else {
        base
    };
    let taken = |cand: &str| existing.iter().any(|s| s.name == cand);
    if !taken(&base) {
        return base;
    }
    for n in 2.. {
        let cand = format!("{base} ({n})");
        if !taken(&cand) {
            return cand;
        }
    }
    unreachable!()
}

/// Appends a new text snippet to `path`, creating it if missing and refusing to overwrite an
/// existing broken or oversized file.
pub fn append_snippet(path: &Path, text: &str) -> Result<Snippet, String> {
    if text.is_empty() {
        return Err("Cannot save an empty entry as a snippet.".into());
    }
    let existing = match OpenOptions::new().read(true).open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "snippets.toml could not be read.".to_owned())?;
            if bytes.len() > MAX_FILE_BYTES {
                return Err("snippets.toml is larger than 256 KiB.".into());
            }
            String::from_utf8(bytes)
                .map_err(|_| "Fix snippets.toml first: it is not valid UTF-8.".to_owned())?
        }
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err("snippets.toml could not be read.".into()),
    };

    let parsed = SnippetsFile::parse(&existing)
        .map_err(|_| "Fix snippets.toml first: it could not be parsed.".to_owned())?;
    let snippet = Snippet {
        name: derive_name(text, &parsed.snippets),
        alias: None,
        text: text.to_owned(),
    };
    let block = toml::to_string(&RawFile {
        snippets: vec![snippet.clone()],
    })
    .map_err(|_| "The snippet could not be formatted as TOML.".to_owned())?;

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() && !updated.ends_with("\n\n") {
        updated.push('\n');
    }
    updated.push_str(&block);

    SnippetsFile::parse(&updated).map_err(|e| format!("The snippet could not be added: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "The settings folder could not be created.".to_owned())?;
    }
    let tmp = path.with_extension("toml.tmp");
    let written = File::create(&tmp)
        .and_then(|mut f| f.write_all(updated.as_bytes()))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return Err("snippets.toml could not be written.".to_owned());
    }
    Ok(snippet)
}
