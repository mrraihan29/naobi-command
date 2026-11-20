//! Offline currency conversion using an immutable `FxRates` snapshot (std-only, zero network).

use crate::format;
use std::collections::BTreeMap;

/// Number formatting convention for displayed currency values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumberFormat {
    #[default]
    Us,
    Id,
}

impl NumberFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "us" => Some(Self::Us),
            "id" => Some(Self::Id),
            _ => None,
        }
    }
}

/// Immutable foreign-exchange rate snapshot (`base`, `date`, `rates` map relative to `base`).
#[derive(Debug, Clone, PartialEq)]
pub struct FxRates {
    pub base: String,
    pub date: String,
    pub rates: BTreeMap<String, f64>,
}

impl FxRates {
    pub fn new(
        base: impl Into<String>,
        date: impl Into<String>,
        rates: BTreeMap<String, f64>,
    ) -> Self {
        let base = base.into().trim().to_ascii_uppercase();
        let date = date.into().trim().to_owned();
        let rates = rates
            .into_iter()
            .filter_map(|(k, v)| {
                let code = k.trim().to_ascii_uppercase();
                (is_currency_code(&code) && v.is_finite() && v > 0.0).then_some((code, v))
            })
            .collect();
        Self { base, date, rates }
    }

    /// Rate of `code` per 1 unit of `self.base`. `self.base` itself always has rate `1.0`.
    pub fn rate(&self, code: &str) -> Option<f64> {
        let upper = code.trim().to_ascii_uppercase();
        if upper == self.base {
            return Some(1.0);
        }
        self.rates
            .get(&upper)
            .copied()
            .filter(|r| r.is_finite() && *r > 0.0)
    }

    /// Parses Frankfurter / `fx-rates.json` JSON (`{"base": "...", "date": "...", "rates": {...}}`).
    pub fn from_json(src: &str) -> Option<Self> {
        let mut p = JsonParser {
            s: src.as_bytes(),
            pos: 0,
            depth: 0,
        };
        p.expect(b'{')?;
        let mut base = None;
        let mut date = None;
        let mut rates = None;
        if !p.eat(b'}') {
            loop {
                let key = p.string()?;
                p.expect(b':')?;
                match key.as_str() {
                    "base" => base = Some(p.string()?),
                    "date" => date = Some(p.string()?),
                    "rates" => rates = Some(p.rates_map()?),
                    _ => p.skip_value()?,
                }
                if p.eat(b',') {
                    continue;
                }
                p.expect(b'}')?;
                break;
            }
        }
        p.skip_ws();
        if p.pos != p.s.len() {
            return None;
        }
        let base = base?.trim().to_ascii_uppercase();
        let date = date?.trim().to_owned();
        let rates = rates?;
        if !is_currency_code(&base)
            || date.is_empty()
            || date.bytes().any(|b| b.is_ascii_control())
            || rates.is_empty()
        {
            return None;
        }
        Some(Self { base, date, rates })
    }

    /// Parses ECB `eurofxref-daily.xml` (`<Cube time='YYYY-MM-DD'><Cube currency='USD' rate='...'/>...`).
    pub fn from_ecb_xml(src: &str) -> Option<Self> {
        let mut date: Option<String> = None;
        let mut rates = BTreeMap::new();
        let mut rest = src;

        while let Some(lt) = rest.find('<') {
            rest = &rest[lt + 1..];
            if let Some(comment) = rest.strip_prefix("!--") {
                let end = comment.find("-->")?;
                rest = &comment[end + 3..];
                continue;
            }
            if rest.starts_with('?') {
                let end = rest.find("?>")?;
                rest = &rest[end + 2..];
                continue;
            }
            let gt = find_tag_end(rest)?;
            let tag_body = rest[..gt].trim();
            rest = &rest[gt + 1..];
            if tag_body.is_empty() || tag_body.starts_with('/') {
                continue;
            }
            let tag_body = tag_body.strip_suffix('/').unwrap_or(tag_body).trim_end();
            let (raw_name, attrs_str) = match tag_body.find(char::is_whitespace) {
                Some(i) => (&tag_body[..i], &tag_body[i..]),
                None => (tag_body, ""),
            };
            let local_name = raw_name
                .rsplit_once(':')
                .map_or(raw_name, |(_, local)| local);
            if local_name != "Cube" {
                continue;
            }
            let (mut time_attr, mut cur_attr, mut rate_attr) = (None, None, None);
            parse_xml_attrs(attrs_str, |k, v| match k {
                "time" => time_attr = Some(v),
                "currency" => cur_attr = Some(v),
                "rate" => rate_attr = Some(v),
                _ => {}
            })?;
            if let Some(t) = time_attr {
                let t = t.trim();
                if t.is_empty() || t.bytes().any(|b| b.is_ascii_control()) {
                    return None;
                }
                if date.is_none() {
                    date = Some(t.to_owned());
                }
            }
            if cur_attr.is_some() || rate_attr.is_some() {
                let code = cur_attr?.trim().to_ascii_uppercase();
                let rate: f64 = rate_attr?.trim().parse().ok()?;
                if !is_currency_code(&code) || !rate.is_finite() || rate <= 0.0 {
                    return None;
                }
                rates.insert(code, rate);
            }
        }

        let date = date?;
        if rates.is_empty() {
            return None;
        }
        Some(Self {
            base: "EUR".to_owned(),
            date,
            rates,
        })
    }

    /// Serializes this snapshot to the `fx-rates.json` cache schema.
    pub fn to_json(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(64 + self.rates.len() * 16);
        out.push_str("{\"base\":\"");
        push_json_escaped(&mut out, &self.base);
        out.push_str("\",\"date\":\"");
        push_json_escaped(&mut out, &self.date);
        out.push_str("\",\"rates\":{");
        for (i, (code, rate)) in self.rates.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "\"{code}\":{rate}");
        }
        out.push_str("}}");
        out
    }
}

fn push_json_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
}

fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, b) in s.bytes().enumerate() {
        match (quote, b) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => quote = Some(b),
            (None, b'>') => return Some(i),
            (None, b'<') => return None,
            _ => {}
        }
    }
    None
}

fn parse_xml_attrs<'a>(mut s: &'a str, mut on_attr: impl FnMut(&'a str, &'a str)) -> Option<()> {
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return Some(());
        }
        let eq = s.find('=')?;
        let key = s[..eq].trim();
        if key.is_empty() {
            return None;
        }
        let after_eq = s[eq + 1..].trim_start();
        let quote = after_eq.chars().next().filter(|&c| c == '\'' || c == '"')?;
        let rest = &after_eq[quote.len_utf8()..];
        let end = rest.find(quote)?;
        on_attr(key, &rest[..end]);
        s = &rest[end + quote.len_utf8()..];
    }
}

/// Result of evaluating a currency conversion query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrencyOutcome {
    Converted(CurrencyConversion),
    MissingRates { from: String, to: String },
}

/// Formatted currency conversion result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrencyConversion {
    pub display: String,
    pub raw: String,
    pub subtitle: String,
}

const COMMON_CURRENCIES: &[&str] = &[
    "AED", "AUD", "BGN", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF",
    "IDR", "ILS", "INR", "ISK", "JPY", "KRW", "MXN", "MYR", "NOK", "NZD", "PHP", "PLN", "RON",
    "RUB", "SAR", "SEK", "SGD", "THB", "TRY", "TWD", "USD", "VND", "ZAR",
];

fn is_currency_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_alphabetic())
}

fn is_common_currency(code: &str) -> bool {
    COMMON_CURRENCIES
        .iter()
        .any(|c| c.eq_ignore_ascii_case(code))
}

fn parse_currency_token(tok: &str) -> Option<String> {
    match tok {
        "$" => Some("USD".to_owned()),
        "€" => Some("EUR".to_owned()),
        "£" => Some("GBP".to_owned()),
        "¥" => Some("JPY".to_owned()),
        s if s.eq_ignore_ascii_case("rp") => Some("IDR".to_owned()),
        s if is_currency_code(s) => Some(s.to_ascii_uppercase()),
        _ => None,
    }
}

fn strip_leading_symbol(s: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = s.strip_prefix('$') {
        Some(("USD", rest))
    } else if let Some(rest) = s.strip_prefix('€') {
        Some(("EUR", rest))
    } else if let Some(rest) = s.strip_prefix('£') {
        Some(("GBP", rest))
    } else if let Some(rest) = s.strip_prefix('¥') {
        Some(("JPY", rest))
    } else if s.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("rp")) {
        Some(("IDR", &s[2..]))
    } else {
        None
    }
}

struct ParsedQuery {
    amount: f64,
    from: String,
    to: String,
    explicit_target: bool,
}

fn parse_query(input: &str, default_currency: &str) -> Option<ParsedQuery> {
    let words: Vec<&str> = input.split_whitespace().collect();
    if words.is_empty() || words.len() > 5 {
        return None;
    }
    let (lhs_words, explicit_to) = if words.len() >= 3
        && (words[words.len() - 2].eq_ignore_ascii_case("to")
            || words[words.len() - 2].eq_ignore_ascii_case("in"))
    {
        let to = parse_currency_token(words[words.len() - 1])?;
        (&words[..words.len() - 2], Some(to))
    } else {
        (&words[..], None)
    };
    if lhs_words.is_empty() || lhs_words.len() > 2 {
        return None;
    }
    let lhs = lhs_words.concat();
    let (amount, from) = parse_amount_and_currency(&lhs)?;
    let explicit_target = explicit_to.is_some();
    let to = explicit_to.unwrap_or_else(|| default_currency.trim().to_ascii_uppercase());
    if !is_currency_code(&to) {
        return None;
    }
    Some(ParsedQuery {
        amount,
        from,
        to,
        explicit_target,
    })
}

fn parse_amount_and_currency(lhs: &str) -> Option<(f64, String)> {
    let (neg, mut rest) = match lhs.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, lhs),
    };
    let mut leading_cur = None;
    if let Some((cur, after)) = strip_leading_symbol(rest) {
        leading_cur = Some(cur);
        rest = after;
    }
    let num_len = rest
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'.')
        .count();
    if num_len == 0 {
        return None;
    }
    let num_str = &rest[..num_len];
    if !num_str.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    let base_val: f64 = num_str.parse().ok()?;
    if !base_val.is_finite() {
        return None;
    }
    let tail = &rest[num_len..];
    let (mult, cur_str) = match tail.as_bytes().first().copied() {
        Some(b'k' | b'K') if leading_cur.is_some() && tail.len() == 1 => (1e3, ""),
        Some(b'm' | b'M') if leading_cur.is_some() && tail.len() == 1 => (1e6, ""),
        Some(b'b' | b'B') if leading_cur.is_some() && tail.len() == 1 => (1e9, ""),
        Some(b'k' | b'K') if leading_cur.is_none() && tail.len() >= 4 => (1e3, &tail[1..]),
        Some(b'm' | b'M') if leading_cur.is_none() && tail.len() >= 4 => (1e6, &tail[1..]),
        Some(b'b' | b'B') if leading_cur.is_none() && tail.len() >= 4 => (1e9, &tail[1..]),
        _ => (1.0, tail),
    };
    let currency = match (leading_cur, cur_str.is_empty()) {
        (Some(cur), true) => cur.to_owned(),
        (Some(_), false) => return None,
        (None, true) => return None,
        (None, false) => parse_currency_token(cur_str)?,
    };
    let value = if neg { -base_val } else { base_val } * mult;
    value.is_finite().then_some((value, currency))
}

/// Evaluate a currency query against an optional `FxRates` snapshot.
/// Returns `None` when the input is not a currency query (or uses an unknown currency).
pub fn evaluate_currency(
    input: &str,
    default_currency: &str,
    number_format: NumberFormat,
    rates: Option<&FxRates>,
) -> Option<CurrencyOutcome> {
    let q = parse_query(input, default_currency)?;
    if !q.explicit_target && q.from == q.to {
        return None;
    }
    let Some(fx) = rates else {
        return (is_common_currency(&q.from) && is_common_currency(&q.to)).then_some(
            CurrencyOutcome::MissingRates {
                from: q.from,
                to: q.to,
            },
        );
    };
    let rate_from = fx.rate(&q.from)?;
    let rate_to = fx.rate(&q.to)?;
    let converted = q.amount * rate_to / rate_from;
    if !converted.is_finite() {
        return None;
    }
    let (raw, formatted_num) = format_amount(converted, number_format);
    let src_amount = format::sig10(q.amount)?;
    Some(CurrencyOutcome::Converted(CurrencyConversion {
        display: format!("{formatted_num} {}", q.to),
        raw,
        subtitle: format!("{src_amount} {} to {} · Rates {}", q.from, q.to, fx.date),
    }))
}

fn format_amount(value: f64, number_format: NumberFormat) -> (String, String) {
    let clean = if value == 0.0 { 0.0 } else { value };
    let raw = if clean == 0.0 || clean.abs() >= 0.01 {
        format!("{clean:.2}")
    } else {
        let s = format!("{clean:.6}");
        let trimmed = s.trim_end_matches('0');
        match trimmed.split_once('.') {
            Some((int_part, frac)) if frac.len() < 2 => format!("{int_part}.{frac:0<2}"),
            _ => trimmed.to_owned(),
        }
    };
    let (neg, unsigned) = match raw.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", raw.as_str()),
    };
    let (int_part, frac_part) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let (thou, dec) = match number_format {
        NumberFormat::Us => (',', '.'),
        NumberFormat::Id => ('.', ','),
    };
    let grouped = group_thousands(int_part, thou);
    let display_num = if frac_part.is_empty() {
        format!("{neg}{grouped}")
    } else {
        format!("{neg}{grouped}{dec}{frac_part}")
    };
    (raw, display_num)
}

fn group_thousands(digits: &str, sep: char) -> String {
    let len = digits.len();
    if len <= 3 {
        return digits.to_owned();
    }
    let mut out = String::with_capacity(len + (len - 1) / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

// --- Minimal std-only JSON parser for fx-rates.json ---

const MAX_JSON_DEPTH: u8 = 16;

struct JsonParser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: u8,
}

impl JsonParser<'_> {
    fn skip_ws(&mut self) {
        while self.s.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, b: u8) -> Option<()> {
        self.eat(b).then_some(())
    }

    fn string(&mut self) -> Option<String> {
        self.expect(b'"')?;
        let start = self.pos;
        let mut out = String::new();
        while let Some(&b) = self.s.get(self.pos) {
            if b == b'"' {
                if out.is_empty() {
                    out.push_str(std::str::from_utf8(&self.s[start..self.pos]).ok()?);
                }
                self.pos += 1;
                return Some(out);
            }
            if b == b'\\' {
                if out.is_empty() {
                    out.push_str(std::str::from_utf8(&self.s[start..self.pos]).ok()?);
                }
                self.pos += 1;
                let esc = *self.s.get(self.pos)?;
                let ch = match esc {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    _ => return None,
                };
                out.push(ch);
                self.pos += 1;
            } else if b.is_ascii_control() {
                return None;
            } else {
                self.pos += 1;
                if !out.is_empty() {
                    // Advance one UTF-8 scalar or byte for plain ASCII after an escape.
                    let s = std::str::from_utf8(&self.s[self.pos - 1..]).ok()?;
                    let ch = s.chars().next()?;
                    out.push(ch);
                    self.pos += ch.len_utf8() - 1;
                }
            }
        }
        None
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_ws();
        let start = self.pos;
        while self
            .s
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_digit() || matches!(*b, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        let v: f64 = std::str::from_utf8(&self.s[start..self.pos])
            .ok()?
            .parse()
            .ok()?;
        v.is_finite().then_some(v)
    }

    fn rates_map(&mut self) -> Option<BTreeMap<String, f64>> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        if self.eat(b'}') {
            return Some(map);
        }
        loop {
            let code = self.string()?.trim().to_ascii_uppercase();
            self.expect(b':')?;
            let rate = self.number()?;
            if !is_currency_code(&code) || !rate.is_finite() || rate <= 0.0 {
                return None;
            }
            map.insert(code, rate);
            if self.eat(b',') {
                continue;
            }
            self.expect(b'}')?;
            break;
        }
        Some(map)
    }

    fn skip_value(&mut self) -> Option<()> {
        if self.depth >= MAX_JSON_DEPTH {
            return None;
        }
        match self.peek()? {
            b'"' => {
                self.string()?;
            }
            b'{' => {
                self.pos += 1;
                self.depth += 1;
                if !self.eat(b'}') {
                    loop {
                        self.string()?;
                        self.expect(b':')?;
                        self.skip_value()?;
                        if !self.eat(b',') {
                            self.expect(b'}')?;
                            break;
                        }
                    }
                }
                self.depth -= 1;
            }
            b'[' => {
                self.pos += 1;
                self.depth += 1;
                if !self.eat(b']') {
                    loop {
                        self.skip_value()?;
                        if !self.eat(b',') {
                            self.expect(b']')?;
                            break;
                        }
                    }
                }
                self.depth -= 1;
            }
            b't' if self.s[self.pos..].starts_with(b"true") => self.pos += 4,
            b'f' if self.s[self.pos..].starts_with(b"false") => self.pos += 5,
            b'n' if self.s[self.pos..].starts_with(b"null") => self.pos += 4,
            _ => {
                self.number()?;
            }
        }
        Some(())
    }
}
