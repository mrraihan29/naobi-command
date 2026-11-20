//! Recursive-descent evaluator. Each value carries an f64 and, while every step stayed an
//! exact integer, an i128 (used by `to hex` / `to bin`).

const MAX_DEPTH: u32 = 64;

#[derive(Clone, Copy)]
pub struct Val {
    pub f: f64,
    pub i: Option<i128>,
}

impl Val {
    fn float(f: f64) -> Self {
        Val { f, i: None }
    }
}

/// A value plus "was a postfix percent": `f` is then the raw `b` of `b%`.
type Term = (Val, bool);

struct P<'a> {
    s: &'a [u8],
    pos: usize,
    depth: u32,
    ops: u32,
    ans: Option<f64>,
}

/// Evaluate a whole expression; also returns how many operators it used (0 = bare number).
pub fn eval(src: &str, ans: Option<f64>) -> Option<(Val, u32)> {
    let mut p = P {
        s: src.as_bytes(),
        pos: 0,
        depth: 0,
        ops: 0,
        ans: ans.filter(|v| v.is_finite()),
    };
    let v = p.expr()?;
    (p.peek().is_none() && v.f.is_finite()).then_some((v, p.ops))
}

fn norm((v, pct): Term) -> Val {
    if pct {
        Val::float(v.f / 100.0)
    } else {
        v
    }
}

fn num(f: f64, i: Option<i128>) -> Option<Val> {
    f.is_finite().then_some(Val { f, i })
}

fn apply(op: u8, a: Val, b: Val) -> Option<Val> {
    let ints = a.i.zip(b.i);
    match op {
        b'+' => num(a.f + b.f, ints.and_then(|(x, y)| x.checked_add(y))),
        b'-' => num(a.f - b.f, ints.and_then(|(x, y)| x.checked_sub(y))),
        b'*' => num(a.f * b.f, ints.and_then(|(x, y)| x.checked_mul(y))),
        // Integer path only when the division is exact.
        b'/' if b.f != 0.0 => num(
            a.f / b.f,
            ints.filter(|&(x, y)| x.checked_rem(y) == Some(0))
                .and_then(|(x, y)| x.checked_div(y)),
        ),
        b'm' if b.f != 0.0 => num(a.f % b.f, ints.and_then(|(x, y)| x.checked_rem(y))),
        b'^' => num(
            a.f.powf(b.f),
            ints.and_then(|(x, y)| u32::try_from(y).ok().and_then(|y| x.checked_pow(y))),
        ),
        _ => None,
    }
}

impl P<'_> {
    fn peek(&mut self) -> Option<u8> {
        while self.s.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        let hit = self.peek() == Some(c);
        self.pos += usize::from(hit);
        hit
    }

    fn eat_mod(&mut self) -> bool {
        self.peek();
        let r = &self.s[self.pos..];
        let hit = r.len() >= 3
            && r[..3].eq_ignore_ascii_case(b"mod")
            && !r.get(3).is_some_and(u8::is_ascii_alphanumeric);
        self.pos += 3 * usize::from(hit);
        hit
    }

    /// Descend one nesting level; `None` past the cap so hostile input cannot overflow the stack.
    fn nest<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn expr(&mut self) -> Option<Val> {
        let mut l = norm(self.term()?);
        loop {
            let op = if self.eat(b'+') {
                b'+'
            } else if self.eat(b'-') {
                b'-'
            } else {
                return Some(l);
            };
            self.ops += 1;
            let (r, pct) = self.term()?;
            // a +/- b% = a +/- a*b/100
            let r = if pct {
                Val::float(l.f * r.f / 100.0)
            } else {
                r
            };
            l = apply(op, l, r)?;
        }
    }

    fn term(&mut self) -> Option<Term> {
        let (mut l, mut pct) = self.unary()?;
        loop {
            let op = if self.eat(b'*') {
                b'*'
            } else if self.eat(b'/') {
                b'/'
            } else if self.eat_mod() {
                b'm'
            } else {
                return Some((l, pct));
            };
            self.ops += 1;
            let r = self.unary()?;
            l = apply(op, norm((l, pct)), norm(r))?;
            pct = false;
        }
    }

    fn unary(&mut self) -> Option<Term> {
        self.nest(|p| {
            if p.eat(b'-') {
                let v = norm(p.unary()?);
                Some((num(-v.f, v.i.and_then(i128::checked_neg))?, false))
            } else if p.eat(b'+') {
                Some((norm(p.unary()?), false))
            } else {
                p.power()
            }
        })
    }

    fn power(&mut self) -> Option<Term> {
        let base = self.atom()?;
        if !self.eat(b'^') {
            return Some(base);
        }
        self.ops += 1;
        let e = self.unary()?; // right-associative; exponent may carry its own sign
        Some((apply(b'^', norm(base), norm(e))?, false))
    }

    fn try_ans(&mut self) -> Option<Option<Val>> {
        self.peek();
        let r = &self.s[self.pos..];
        if r.len() >= 3
            && r[..3].eq_ignore_ascii_case(b"ans")
            && !r
                .get(3)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.pos += 3;
            Some(self.ans.map(|f| Val {
                f,
                i: (f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0).then_some(f as i128),
            }))
        } else {
            None
        }
    }

    fn atom(&mut self) -> Option<Term> {
        let v = if self.eat(b'(') {
            let v = self.nest(Self::expr)?;
            if !self.eat(b')') {
                return None;
            }
            v
        } else if let Some(ans) = self.try_ans() {
            ans?
        } else {
            self.number()?
        };
        if self.eat(b'%') {
            self.ops += 1;
            return Some((Val::float(v.f), true));
        }
        Some((v, false))
    }

    fn eat_currency(&mut self) -> bool {
        self.peek();
        let rest = &self.s[self.pos..];
        let len = ["$", "€", "£", "¥"]
            .into_iter()
            .map(str::as_bytes)
            .find(|p| rest.starts_with(p))
            .map(<[u8]>::len)
            .or_else(|| {
                rest.get(..2)
                    .filter(|b| b.eq_ignore_ascii_case(b"rp"))
                    .map(|_| 2)
            });
        if let Some(n) = len {
            self.pos += n;
            self.peek();
            true
        } else {
            false
        }
    }

    fn number(&mut self) -> Option<Val> {
        let currency = self.eat_currency();
        let rest = &self.s[self.pos..];
        let radix = if currency {
            10
        } else {
            match rest {
                [b'0', b'x' | b'X', ..] => 16,
                [b'0', b'b' | b'B', ..] => 2,
                _ => 10,
            }
        };
        let skip = if radix == 10 { 0 } else { 2 };
        let len = rest[skip..]
            .iter()
            .take_while(|c| {
                if radix == 10 {
                    c.is_ascii_digit() || **c == b'.'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
            .count();
        let text = std::str::from_utf8(&rest[skip..skip + len]).ok()?;
        self.pos += skip + len;
        if radix != 10 {
            let i = i128::from_str_radix(text, radix).ok()?;
            return Some(Val {
                f: i as f64,
                i: Some(i),
            });
        }
        let f: f64 = text.parse().ok()?;
        Some(Val {
            f,
            i: text.parse().ok(),
        })
    }
}
