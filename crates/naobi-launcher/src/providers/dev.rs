//! Offline developer micro-tools in Root search (Issue #5): `uuid`, `ulid`, `b64`, `url`,
//! `sha256`, `json`, `wc`. Pure Rust `std`, never logs inputs or clipboard payloads (D-020).
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// Host capabilities required by developer micro-tools.
pub trait DevEnv: Send + Sync {
    fn clipboard_text(&self) -> Option<String>;
    fn random16(&self) -> Option<[u8; 16]>;
    fn now_ms(&self) -> i64;
}

#[derive(Default)]
struct CachedOutput {
    primary: Zeroizing<String>,
    secondary: Zeroizing<String>,
    error: Option<(String, String)>,
}

pub struct DevSource {
    env: Box<dyn DevEnv>,
    state: Mutex<CachedOutput>,
}

impl DevSource {
    pub fn new(env: Box<dyn DevEnv>) -> Self {
        Self {
            env,
            state: Mutex::new(CachedOutput::default()),
        }
    }
}

impl CommandSource for DevSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let trimmed = q.text.trim();
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *guard = CachedOutput::default();

        let (cmd, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((c, r)) => (c, r.trim_start()),
            None => (trimmed, ""),
        };

        let result = match cmd.to_ascii_lowercase().as_str() {
            "uuid" if rest.is_empty() => match self.env.random16() {
                Some(bytes) => Ok(("dev:uuid", format_uuid_v4(bytes), "UUID v4".into())),
                None => Err(("UUID", "Could not read system randomness.")),
            },
            "ulid" if rest.is_empty() => match self.env.random16() {
                Some(bytes) => Ok((
                    "dev:ulid",
                    format_ulid(self.env.now_ms(), bytes),
                    "ULID".into(),
                )),
                None => Err(("ULID", "Could not read system randomness.")),
            },
            "b64" => {
                let (decode, arg) = split_encode_decode(rest);
                run_codec(
                    &*self.env,
                    arg,
                    "Base64",
                    if decode { "dev:b64d" } else { "dev:b64" },
                    if decode {
                        "Base64 decode"
                    } else {
                        "Base64 encode"
                    },
                    if decode {
                        base64_decode
                    } else {
                        |s| Ok(base64_encode(s.as_bytes()))
                    },
                )
            }
            "b64d" => run_codec(
                &*self.env,
                rest,
                "Base64",
                "dev:b64d",
                "Base64 decode",
                base64_decode,
            ),
            "url" => {
                let (decode, arg) = split_encode_decode(rest);
                run_codec(
                    &*self.env,
                    arg,
                    "URL",
                    if decode { "dev:urld" } else { "dev:url" },
                    if decode { "URL decode" } else { "URL encode" },
                    if decode {
                        url_decode
                    } else {
                        |s| Ok(naobi_core::quicklink::percent_encode(s))
                    },
                )
            }
            "urld" => run_codec(
                &*self.env,
                rest,
                "URL",
                "dev:urld",
                "URL decode",
                url_decode,
            ),
            "sha256" => run_codec(&*self.env, rest, "SHA-256", "dev:sha256", "SHA-256", |s| {
                Ok(sha256_hex(s.as_bytes()))
            }),
            "json" => return self.json_items(q, rest, &mut guard),
            "wc" => run_codec(
                &*self.env,
                rest,
                "Word count",
                "dev:wc",
                "Word count",
                |s| Ok(word_count(s)),
            ),
            _ => return Arc::from(Vec::new()),
        };

        match result {
            Ok((id, value, subtitle)) => {
                let title = single_line(&value);
                guard.primary = Zeroizing::new(value);
                vec![Item {
                    id: ItemId(id.into()),
                    title,
                    subtitle,
                    keywords: Vec::new(),
                    alias: Some(q.text.clone()),
                    kind: ItemKind::Utility,
                    primary: ActionId("copy".into()),
                    secondary: None,
                }]
                .into()
            }
            Err((subject, message)) => {
                guard.error = Some((subject.into(), message.into()));
                vec![error_item(q, subject, message)].into()
            }
        }
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let act = |aid: &str, label: &str| Action {
            id: ActionId(aid.into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        };
        match id.0.as_str() {
            "dev:error" => Vec::new(),
            "dev:json.format" => vec![
                act("copy", "Copy Formatted JSON"),
                act("secondary", "Copy Minified JSON"),
            ],
            "dev:json.minify" => vec![
                act("copy", "Copy Minified JSON"),
                act("secondary", "Copy Formatted JSON"),
            ],
            _ => vec![act("copy", "Copy")],
        }
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if id.0 == "dev:error" {
            if let Some((subject, message)) = &guard.error {
                return Outcome::Failed(Notice {
                    subject: subject.clone(),
                    message: message.clone(),
                    kind: NoticeKind::Error,
                });
            }
        }
        let text = if action.0 == "secondary" {
            &guard.secondary
        } else {
            &guard.primary
        };
        if !text.is_empty() {
            cx.copy_text(text.to_string());
        }
        Outcome::Done
    }
}

impl DevSource {
    fn json_items(&self, q: &Query, rest: &str, guard: &mut CachedOutput) -> Arc<[Item]> {
        let (sub, tail) = match rest.split_once(char::is_whitespace) {
            Some((s, t)) => (s, t.trim_start()),
            None => (rest, ""),
        };
        let (minify_first, arg) =
            if sub.eq_ignore_ascii_case("minify") || sub.eq_ignore_ascii_case("min") {
                (true, tail)
            } else if sub.eq_ignore_ascii_case("format") || sub.eq_ignore_ascii_case("fmt") {
                (false, tail)
            } else {
                (false, rest)
            };
        let (input, from_clipboard) = match resolve_input(&*self.env, arg) {
            Ok(v) => v,
            Err(msg) => {
                guard.error = Some(("JSON".into(), msg.into()));
                return vec![error_item(q, "JSON", msg)].into();
            }
        };
        let (formatted, minified) = match format_and_minify_json(&input) {
            Ok(pair) => pair,
            Err(msg) => {
                guard.error = Some(("JSON".into(), msg.into()));
                return vec![error_item(q, "JSON", msg)].into();
            }
        };
        let (id, primary, secondary, label) = if minify_first {
            ("dev:json.minify", minified, formatted, "Minify JSON")
        } else {
            ("dev:json.format", formatted, minified, "Format JSON")
        };
        let title = single_line(&primary);
        guard.primary = Zeroizing::new(primary);
        guard.secondary = Zeroizing::new(secondary);
        let subtitle = if from_clipboard {
            format!("{label} · from clipboard")
        } else {
            label.to_owned()
        };
        vec![Item {
            id: ItemId(id.into()),
            title,
            subtitle,
            keywords: Vec::new(),
            alias: Some(q.text.clone()),
            kind: ItemKind::Utility,
            primary: ActionId("copy".into()),
            secondary: Some(ActionId("secondary".into())),
        }]
        .into()
    }
}

fn error_item(q: &Query, subject: &str, message: &str) -> Item {
    Item {
        id: ItemId("dev:error".into()),
        title: format!("{subject}: {message}"),
        subtitle: "Press Enter for details".into(),
        keywords: Vec::new(),
        alias: Some(q.text.clone()),
        kind: ItemKind::Utility,
        primary: ActionId("copy".into()),
        secondary: None,
    }
}

fn format_uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn format_ulid(now_ms: i64, rand: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let ts = (now_ms.max(0) as u64) & ((1u64 << 48) - 1);
    let mut out = String::with_capacity(26);
    for shift in (0..10).rev() {
        let idx = ((ts >> (shift * 5)) & 0x1f) as usize;
        out.push(char::from(ALPHABET[idx]));
    }
    let r80 = u128::from_be_bytes([
        0, 0, 0, 0, 0, 0, rand[0], rand[1], rand[2], rand[3], rand[4], rand[5], rand[6], rand[7],
        rand[8], rand[9],
    ]);
    for shift in (0..16).rev() {
        let idx = ((r80 >> (shift * 5)) & 0x1f) as usize;
        out.push(char::from(ALPHABET[idx]));
    }
    out
}

const MAX_INPUT_BYTES: usize = 1024 * 1024;

fn split_encode_decode(rest: &str) -> (bool, &str) {
    let (sub, tail) = match rest.split_once(char::is_whitespace) {
        Some((s, t)) => (s, t.trim_start()),
        None => (rest, ""),
    };
    if sub.eq_ignore_ascii_case("decode") {
        (true, tail)
    } else if sub.eq_ignore_ascii_case("encode") {
        (false, tail)
    } else {
        (false, rest)
    }
}

fn resolve_input(
    env: &dyn DevEnv,
    inline: &str,
) -> Result<(Zeroizing<String>, bool), &'static str> {
    let input = if !inline.is_empty() {
        (Zeroizing::new(inline.to_owned()), false)
    } else {
        let clip = env
            .clipboard_text()
            .filter(|t| !t.is_empty())
            .ok_or("Clipboard has no plaintext.")?;
        (Zeroizing::new(clip), true)
    };
    if input.0.len() > MAX_INPUT_BYTES {
        return Err("Input exceeds 1 MB limit.");
    }
    Ok(input)
}

fn run_codec(
    env: &dyn DevEnv,
    inline: &str,
    subject: &'static str,
    id: &'static str,
    label: &'static str,
    transform: fn(&str) -> Result<String, &'static str>,
) -> Result<(&'static str, String, String), (&'static str, &'static str)> {
    let (input, from_clipboard) = resolve_input(env, inline).map_err(|msg| (subject, msg))?;
    let out = transform(&input).map_err(|msg| (subject, msg))?;
    let subtitle = if from_clipboard {
        format!("{label} · from clipboard")
    } else {
        label.to_owned()
    };
    Ok((id, out, subtitle))
}

fn single_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(200));
    let mut prev_ws = false;
    for ch in s.chars().take(200) {
        if matches!(ch, '\r' | '\n' | '\t') {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            prev_ws = false;
            out.push(ch);
        }
    }
    out
}

fn base64_encode(bytes: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = *chunk.get(1).unwrap_or(&0) as usize;
        let b2 = *chunk.get(2).unwrap_or(&0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(TBL[(triple >> 18) & 0x3f]));
        out.push(char::from(TBL[(triple >> 12) & 0x3f]));
        out.push(if chunk.len() > 1 {
            char::from(TBL[(triple >> 6) & 0x3f])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(TBL[triple & 0x3f])
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Result<String, &'static str> {
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() {
        return Err("Input is not valid Base64.");
    }
    if !clean.len().is_multiple_of(4) {
        return Err("Malformed Base64 length.");
    }
    let pad = clean.iter().rev().take(2).filter(|&&b| b == b'=').count();
    if clean[..clean.len() - pad].contains(&b'=') {
        return Err("Malformed Base64 padding.");
    }
    let val = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    let (chunks, _) = clean.as_chunks::<4>();
    for (i, chunk) in chunks.iter().enumerate() {
        let last = (i + 1) * 4 == clean.len();
        let a = val(chunk[0]).ok_or("Malformed Base64 character.")?;
        let b = val(chunk[1]).ok_or("Malformed Base64 character.")?;
        if last && pad == 2 {
            if (b & 0x0f) != 0 {
                return Err("Malformed Base64 padding bits.");
            }
            out.push((a << 2) | (b >> 4));
        } else if last && pad == 1 {
            let c = val(chunk[2]).ok_or("Malformed Base64 character.")?;
            if (c & 0x03) != 0 {
                return Err("Malformed Base64 padding bits.");
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        } else {
            let c = val(chunk[2]).ok_or("Malformed Base64 character.")?;
            let d = val(chunk[3]).ok_or("Malformed Base64 character.")?;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
        }
    }
    String::from_utf8(out).map_err(|_| "Decoded Base64 is not valid UTF-8.")
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn url_decode(s: &str) -> Result<String, &'static str> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes
                .get(i + 1)
                .copied()
                .and_then(hex_val)
                .ok_or("Malformed percent-encoding.")?;
            let lo = bytes
                .get(i + 2)
                .copied()
                .and_then(hex_val)
                .ok_or("Malformed percent-encoding.")?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Decoded URL text is not valid UTF-8.")
}

fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, delta) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(delta);
        }
    }
    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
    )
}

const MAX_JSON_DEPTH: usize = 128;

enum Json<'a> {
    Null,
    Bool(bool),
    Raw(&'a str),
    Array(Vec<Json<'a>>),
    Object(Vec<(&'a str, Json<'a>)>),
}

struct JsonParser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn parse_root(&mut self) -> Result<Json<'a>, &'static str> {
        self.skip_ws();
        let val = self.parse_value(0)?;
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err("Unexpected trailing characters in JSON.");
        }
        Ok(val)
    }

    fn parse_value(&mut self, depth: usize) -> Result<Json<'a>, &'static str> {
        if depth > MAX_JSON_DEPTH {
            return Err("JSON nesting exceeds maximum depth.");
        }
        self.skip_ws();
        match self.bytes.get(self.pos) {
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b'"') => self.parse_string().map(Json::Raw),
            Some(b't') => self.expect_lit(b"true", Json::Bool(true)),
            Some(b'f') => self.expect_lit(b"false", Json::Bool(false)),
            Some(b'n') => self.expect_lit(b"null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(Json::Raw),
            Some(_) => Err("Unexpected token in JSON."),
            None => Err("Unexpected end of JSON."),
        }
    }

    fn expect_lit(&mut self, lit: &[u8], val: Json<'a>) -> Result<Json<'a>, &'static str> {
        if self.bytes.get(self.pos..self.pos + lit.len()) == Some(lit) {
            self.pos += lit.len();
            Ok(val)
        } else {
            Err("Invalid literal in JSON.")
        }
    }

    fn parse_string(&mut self) -> Result<&'a str, &'static str> {
        let start = self.pos;
        self.pos += 1; // opening '"'
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'"' => {
                    self.pos += 1;
                    return Ok(&self.src[start..self.pos]);
                }
                b'\\' => {
                    self.pos += 1;
                    match self.bytes.get(self.pos) {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.pos += 1;
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            for _ in 0..4 {
                                match self.bytes.get(self.pos) {
                                    Some(c) if c.is_ascii_hexdigit() => self.pos += 1,
                                    _ => return Err("Invalid \\u escape in JSON string."),
                                }
                            }
                        }
                        _ => return Err("Invalid escape in JSON string."),
                    }
                }
                0x00..=0x1F => return Err("Unescaped control character in JSON string."),
                _ => self.pos += 1,
            }
        }
        Err("Unterminated JSON string.")
    }

    fn parse_number(&mut self) -> Result<&'a str, &'static str> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err("Invalid number in JSON."),
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            if !matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                return Err("Invalid fraction in JSON number.");
            }
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                return Err("Invalid exponent in JSON number.");
            }
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        Ok(&self.src[start..self.pos])
    }

    fn parse_array(&mut self, depth: usize) -> Result<Json<'a>, &'static str> {
        self.pos += 1; // '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value(depth)?);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err("Expected ',' or ']' in JSON array."),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Json<'a>, &'static str> {
        self.pos += 1; // '{'
        self.skip_ws();
        let mut pairs = Vec::new();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Json::Object(pairs));
        }
        loop {
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err("Expected string key in JSON object.");
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err("Expected ':' after JSON key.");
            }
            self.pos += 1;
            let val = self.parse_value(depth)?;
            pairs.push((key, val));
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(pairs));
                }
                _ => return Err("Expected ',' or '}' in JSON object."),
            }
        }
    }
}

fn write_json(val: &Json<'_>, out: &mut String, indent: Option<usize>) {
    match val {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Raw(s) => out.push_str(s),
        Json::Array(items) if items.is_empty() => out.push_str("[]"),
        Json::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if let Some(d) = indent {
                    out.push('\n');
                    for _ in 0..(d + 1) * 2 {
                        out.push(' ');
                    }
                }
                write_json(item, out, indent.map(|d| d + 1));
            }
            if let Some(d) = indent {
                out.push('\n');
                for _ in 0..d * 2 {
                    out.push(' ');
                }
            }
            out.push(']');
        }
        Json::Object(pairs) if pairs.is_empty() => out.push_str("{}"),
        Json::Object(pairs) => {
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if let Some(d) = indent {
                    out.push('\n');
                    for _ in 0..(d + 1) * 2 {
                        out.push(' ');
                    }
                }
                out.push_str(k);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_json(v, out, indent.map(|d| d + 1));
            }
            if let Some(d) = indent {
                out.push('\n');
                for _ in 0..d * 2 {
                    out.push(' ');
                }
            }
            out.push('}');
        }
    }
}

fn format_and_minify_json(src: &str) -> Result<(String, String), &'static str> {
    let val = JsonParser::new(src).parse_root()?;
    let mut formatted = String::with_capacity(src.len());
    write_json(&val, &mut formatted, Some(0));
    let mut minified = String::with_capacity(src.len());
    write_json(&val, &mut minified, None);
    Ok((formatted, minified))
}

fn word_count(s: &str) -> String {
    let words = s.split_whitespace().count();
    let chars = s.chars().count();
    let lines = if s.is_empty() {
        0
    } else {
        s.lines().count().max(1)
    };
    let w_unit = if words == 1 { "word" } else { "words" };
    let c_unit = if chars == 1 { "char" } else { "chars" };
    let l_unit = if lines == 1 { "line" } else { "lines" };
    format!("{words} {w_unit}, {chars} {c_unit}, {lines} {l_unit}")
}
