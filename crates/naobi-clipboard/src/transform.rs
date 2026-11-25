//! Local, zero-dependency clipboard transforms (`CSV -> JSON`, `XML -> JSON`, `HTML -> Markdown`)
//! and temporary file materialization/cleanup for `Paste as .txt/.png file`.
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const TEMP_SUBDIR: &str = "NaobiCommand";
pub const TEMP_TTL: Duration = Duration::from_secs(3600);

pub fn default_temp_dir() -> PathBuf {
    std::env::temp_dir().join(TEMP_SUBDIR)
}

/// Removes files inside `dir` whose age exceeds `max_age`. Missing directories are ignored.
pub fn prune_temp_dir(dir: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if now.duration_since(modified).is_ok_and(|age| age > max_age) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Writes `bytes` into a fresh temporary file inside `dir` (`clip-<now_ms>-<pid>.<ext>`) after
/// pruning expired temp files.
pub fn write_temp_file(dir: &Path, ext: &str, bytes: &[u8], now_ms: i64) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    prune_temp_dir(dir, TEMP_TTL);
    let pid = std::process::id();
    for attempt in 0..100u32 {
        let name = if attempt == 0 {
            format!("clip-{now_ms}-{pid}.{ext}")
        } else {
            format!("clip-{now_ms}-{pid}-{attempt}.{ext}")
        };
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, bytes)?;
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate temporary file",
    ))
}

fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn parse_csv_rows(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    if !(row.len() == 1 && row[0].is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    if !(row.len() == 1 && row[0].is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Converts CSV with a header row into pretty JSON array of objects.
pub fn csv_to_json(input: &str) -> Result<String, &'static str> {
    let rows = parse_csv_rows(input.trim());
    if rows.len() < 2 || rows[0].is_empty() {
        return Err("Clipboard text does not look like CSV with a header row.");
    }
    let headers = &rows[0];
    let mut out = String::from("[\n");
    for (ri, row) in rows[1..].iter().enumerate() {
        out.push_str("  {\n");
        for (ci, key) in headers.iter().enumerate() {
            out.push_str("    ");
            push_json_str(&mut out, key.trim());
            out.push_str(": ");
            let val = row.get(ci).map(String::as_str).unwrap_or("");
            push_json_str(&mut out, val);
            if ci + 1 < headers.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }");
        if ri + 2 < rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push(']');
    Ok(out)
}

#[derive(Debug)]
struct XmlNode {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<XmlNode>,
    text: String,
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

struct XmlParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> XmlParser<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn skip_misc(&mut self) {
        loop {
            self.skip_ws();
            if self.rest().starts_with("<?") {
                if let Some(end) = self.rest().find("?>") {
                    self.pos += end + 2;
                    continue;
                }
            }
            if self.rest().starts_with("<!--") {
                if let Some(end) = self.rest().find("-->") {
                    self.pos += end + 3;
                    continue;
                }
            }
            break;
        }
    }

    fn read_name(&mut self) -> Option<String> {
        let start = self.pos;
        for c in self.rest().chars() {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.') {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        (self.pos > start).then(|| self.src[start..self.pos].to_owned())
    }

    fn parse_node(&mut self) -> Result<XmlNode, &'static str> {
        self.skip_misc();
        if !self.rest().starts_with('<') || self.rest().starts_with("</") {
            return Err("Clipboard text is not valid XML.");
        }
        self.pos += 1;
        let name = self.read_name().ok_or("Clipboard text is not valid XML.")?;
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.rest().starts_with("/>") {
                self.pos += 2;
                return Ok(XmlNode {
                    name,
                    attrs,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            if self.rest().starts_with('>') {
                self.pos += 1;
                break;
            }
            let attr_name = self.read_name().ok_or("Clipboard text is not valid XML.")?;
            self.skip_ws();
            if !self.rest().starts_with('=') {
                return Err("Clipboard text is not valid XML.");
            }
            self.pos += 1;
            self.skip_ws();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|&q| q == '"' || q == '\'')
                .ok_or("Clipboard text is not valid XML.")?;
            self.pos += 1;
            let end = self
                .rest()
                .find(quote)
                .ok_or("Clipboard text is not valid XML.")?;
            let val = decode_entities(&self.rest()[..end]);
            self.pos += end + 1;
            attrs.push((attr_name, val));
        }

        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            if self.rest().is_empty() {
                return Err("Clipboard text has an unclosed XML element.");
            }
            if self.rest().starts_with("</") {
                self.pos += 2;
                let close = self.read_name().ok_or("Clipboard text is not valid XML.")?;
                self.skip_ws();
                if close != name || !self.rest().starts_with('>') {
                    return Err("Clipboard text has mismatched XML tags.");
                }
                self.pos += 1;
                break;
            }
            if self.rest().starts_with("<!--") {
                let end = self
                    .rest()
                    .find("-->")
                    .ok_or("Clipboard text has an unclosed XML comment.")?;
                self.pos += end + 3;
                continue;
            }
            if self.rest().starts_with('<') {
                children.push(self.parse_node()?);
            } else {
                let next_lt = self.rest().find('<').unwrap_or(self.rest().len());
                text.push_str(&decode_entities(&self.rest()[..next_lt]));
                self.pos += next_lt;
            }
        }
        Ok(XmlNode {
            name,
            attrs,
            children,
            text: text.trim().to_owned(),
        })
    }
}

fn write_xml_value(out: &mut String, node: &XmlNode, indent: usize) {
    let pad = " ".repeat(indent);
    if node.attrs.is_empty() && node.children.is_empty() {
        push_json_str(out, &node.text);
        return;
    }
    out.push_str("{\n");
    let mut entries: Vec<(String, String)> = Vec::new();
    for (k, v) in &node.attrs {
        let mut s = String::new();
        push_json_str(&mut s, v);
        entries.push((format!("@{k}"), s));
    }
    if !node.text.is_empty() {
        let mut s = String::new();
        push_json_str(&mut s, &node.text);
        entries.push(("#text".to_owned(), s));
    }
    let mut grouped: Vec<(&str, Vec<&XmlNode>)> = Vec::new();
    for ch in &node.children {
        if let Some((_, list)) = grouped.iter_mut().find(|(n, _)| *n == ch.name) {
            list.push(ch);
        } else {
            grouped.push((&ch.name, vec![ch]));
        }
    }
    for (name, list) in grouped {
        let mut val = String::new();
        if list.len() == 1 {
            write_xml_value(&mut val, list[0], indent + 2);
        } else {
            val.push_str("[\n");
            for (i, child) in list.iter().enumerate() {
                val.push_str(&" ".repeat(indent + 4));
                write_xml_value(&mut val, child, indent + 4);
                if i + 1 < list.len() {
                    val.push(',');
                }
                val.push('\n');
            }
            val.push_str(&" ".repeat(indent + 2));
            val.push(']');
        }
        entries.push((name.to_owned(), val));
    }
    for (i, (k, v)) in entries.iter().enumerate() {
        out.push_str(&" ".repeat(indent + 2));
        push_json_str(out, k);
        out.push_str(": ");
        out.push_str(v);
        if i + 1 < entries.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&pad);
    out.push('}');
}

/// Converts well-formed XML text into a pretty JSON object.
pub fn xml_to_json(input: &str) -> Result<String, &'static str> {
    let mut p = XmlParser {
        src: input.trim(),
        pos: 0,
    };
    let root = p.parse_node()?;
    p.skip_misc();
    if !p.rest().is_empty() {
        return Err("Clipboard text has trailing data after XML root.");
    }
    let mut out = String::from("{\n  ");
    push_json_str(&mut out, &root.name);
    out.push_str(": ");
    write_xml_value(&mut out, &root, 2);
    out.push_str("\n}");
    Ok(out)
}

/// Converts HTML (including Windows `CF_HTML` fragment markers) into clean Markdown.
pub fn html_to_markdown(input: &str) -> Result<String, &'static str> {
    let body = if let (Some(start), Some(end)) = (
        input.find("<!--StartFragment-->"),
        input.find("<!--EndFragment-->"),
    ) {
        let from = start + "<!--StartFragment-->".len();
        if from <= end {
            &input[from..end]
        } else {
            input
        }
    } else {
        input
    };
    if !body.contains('<') || !body.contains('>') {
        return Err("Clipboard text does not look like HTML.");
    }
    let mut out = String::new();
    let mut rest = body;
    let mut link_href: Option<String> = None;
    let mut link_text = String::new();

    while let Some(lt) = rest.find('<') {
        let plain = decode_entities(&rest[..lt]);
        if link_href.is_some() {
            link_text.push_str(&plain);
        } else {
            out.push_str(&plain);
        }
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else {
            out.push_str(&rest[lt..]);
            rest = "";
            break;
        };
        let tag_raw = after[..gt].trim();
        rest = &after[gt + 1..];
        let lower = tag_raw.to_ascii_lowercase();
        let name = lower
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let closing = lower.starts_with('/');

        match (name, closing) {
            ("h1", false) => out.push_str("\n# "),
            ("h2", false) => out.push_str("\n## "),
            ("h3", false) => out.push_str("\n### "),
            ("h4", false) => out.push_str("\n#### "),
            ("h5", false) => out.push_str("\n##### "),
            ("h6", false) => out.push_str("\n###### "),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div", true) => out.push('\n'),
            ("br", _) => out.push('\n'),
            ("li", false) => out.push_str("\n- "),
            ("strong" | "b", _) => out.push_str("**"),
            ("em" | "i", _) => out.push('*'),
            ("code", _) => out.push('`'),
            ("pre", false) => out.push_str("\n```\n"),
            ("pre", true) => out.push_str("\n```\n"),
            ("a", false) => {
                if let Some(href) = extract_href(tag_raw) {
                    link_href = Some(href);
                    link_text.clear();
                }
            }
            ("a", true) => {
                if let Some(href) = link_href.take() {
                    let _ = write!(out, "[{}]({href})", link_text.trim());
                    link_text.clear();
                }
            }
            _ => {}
        }
    }
    if !rest.is_empty() {
        out.push_str(&decode_entities(rest));
    }
    let cleaned: Vec<&str> = out.lines().map(str::trim_end).collect::<Vec<_>>();
    let joined = cleaned.join("\n").trim().to_owned();
    if joined.is_empty() {
        return Err("Clipboard HTML produced empty Markdown.");
    }
    Ok(joined)
}

fn extract_href(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let idx = lower.find("href=")?;
    let after = tag[idx + 5..].trim_start();
    let quote = after.chars().next().filter(|&q| q == '"' || q == '\'')?;
    let rest = &after[1..];
    let end = rest.find(quote)?;
    Some(decode_entities(&rest[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_xml_and_html_converters_transform_cleanly() {
        let json = csv_to_json("name,role\n\"Raihan, M\",Owner\n").unwrap();
        assert!(json.contains("\"name\": \"Raihan, M\""));
        assert!(json.contains("\"role\": \"Owner\""));

        let xjson = xml_to_json("<root id=\"1\"><item>a</item><item>b</item></root>").unwrap();
        assert!(xjson.contains("\"@id\": \"1\""));
        assert!(xjson.contains("\"item\": ["));

        let md = html_to_markdown(
            "Version:0.9\n<!--StartFragment--><h2>Title</h2><p>Hi <strong>you</strong> <a href=\"https://naobi.dev\">link</a></p><!--EndFragment-->",
        )
        .unwrap();
        assert_eq!(md, "## Title\nHi **you** [link](https://naobi.dev)");
    }

    #[test]
    fn temp_file_write_and_prune() {
        let dir = std::env::temp_dir().join(format!("naobi-clip-tmp-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = write_temp_file(&dir, "txt", b"hello", 12345).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        // Fresh file survives 1h pruning.
        prune_temp_dir(&dir, TEMP_TTL);
        assert!(path.exists());
        // Pruning with ZERO max age deletes immediately.
        prune_temp_dir(&dir, Duration::ZERO);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
