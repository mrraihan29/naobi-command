/// 10 significant digits, trailing zeros trimmed. `None` for NaN or infinity.
pub fn sig10(f: f64) -> Option<String> {
    if !f.is_finite() {
        return None;
    }
    if f == 0.0 {
        return Some("0".into());
    }
    let sci = format!("{f:.9e}");
    let (mant, exp) = sci.split_once('e')?;
    let e: i32 = exp.parse().ok()?;
    if (-5..=9).contains(&e) {
        Some(trim(format!("{:.*}", (9 - e).max(0) as usize, f)))
    } else {
        Some(format!("{}e{e}", trim(mant.to_string())))
    }
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}
