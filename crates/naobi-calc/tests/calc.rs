use naobi_calc::{evaluate, Kind};

fn text(input: &str) -> Option<String> {
    evaluate(input).map(|o| o.text)
}

// @R-014
#[test]
fn r014_expressions_evaluate() {
    assert_eq!(text("2^10 + 15%").as_deref(), Some("1177.6"));
    assert_eq!(text("1 + 2 * 3").as_deref(), Some("7"));
    assert_eq!(text("(1 + 2) * 3").as_deref(), Some("9"));
    assert_eq!(text("2^3^2").as_deref(), Some("512"));
    assert_eq!(text("-2^2").as_deref(), Some("-4"));
    assert_eq!(text("10 mod 3").as_deref(), Some("1"));
    assert_eq!(text("7 / 2").as_deref(), Some("3.5"));
    assert_eq!(evaluate("1+1").map(|o| o.kind), Some(Kind::Number));
}

// @R-014
#[test]
fn r014_unit_conversions() {
    let km = evaluate("5 km to mi").expect("km to mi");
    assert_eq!(km.text, "3.106855961 mi");
    assert_eq!(km.kind, Kind::Conversion);
    assert_eq!(text("100 F to C").as_deref(), Some("37.77777778 C"));
    assert_eq!(text("0 C in K").as_deref(), Some("273.15 K"));
    assert_eq!(text("5KM TO MI").as_deref(), Some("3.106855961 MI"));
    assert_eq!(text("1 gal to l").as_deref(), Some("3.785411784 l"));
    assert_eq!(text("36 km/h to m/s").as_deref(), Some("10 m/s"));
    assert_eq!(text("2 h to min").as_deref(), Some("120 min"));
    assert_eq!(text("1 ha to m2").as_deref(), Some("10000 m2"));
    assert_eq!(text("16 oz to lb").as_deref(), Some("1 lb"));
    assert_eq!(
        text("1 kilometers to meters").as_deref(),
        Some("1000 meters")
    );
    assert_eq!(text("12 in to ft").as_deref(), Some("1 ft"));
}

#[test]
fn d017_ten_significant_digits() {
    assert_eq!(text("1/3").as_deref(), Some("0.3333333333"));
    assert_eq!(text("0.1 + 0.2").as_deref(), Some("0.3"));
    assert_eq!(text("2/3").as_deref(), Some("0.6666666667"));
    assert_eq!(text("10 * 1.5").as_deref(), Some("15"));
    assert_eq!(text("2^100").as_deref(), Some("1.2676506e30"));
    assert_eq!(text("1/3 * 1000000").as_deref(), Some("333333.3333"));
}

#[test]
fn d017_percent_semantics() {
    assert_eq!(text("200 + 10%").as_deref(), Some("220"));
    assert_eq!(text("200 - 10%").as_deref(), Some("180"));
    assert_eq!(text("200 * 10%").as_deref(), Some("20"));
    assert_eq!(text("15%").as_deref(), Some("0.15"));
    assert_eq!(text("100 + 10% + 10%").as_deref(), Some("121"));
}

#[test]
fn d017_hex_bin_exact_above_2_pow_53() {
    assert_eq!(text("255 to hex").as_deref(), Some("0xFF"));
    assert_eq!(text("10 to bin").as_deref(), Some("0b1010"));
    assert_eq!(text("0xFF + 1").as_deref(), Some("256"));
    assert_eq!(text("0b101 * 2").as_deref(), Some("10"));
    assert_eq!(text("2^53 + 1 to hex").as_deref(), Some("0x20000000000001"));
    assert_eq!(
        text("2^100 to hex").as_deref(),
        Some("0x10000000000000000000000000")
    );
    assert_eq!(text("-255 to hex").as_deref(), Some("-0xFF"));
    assert_eq!(
        text("0x1FFFFFFFFFFFFF + 2 to bin").map(|s| s.len()),
        Some(2 + 54)
    );
    assert_eq!(evaluate("7 to hex").map(|o| o.kind), Some(Kind::Base));
    assert_eq!(text("2.5 to hex"), None);
}

#[test]
fn d017_data_size_decimal_vs_binary() {
    assert_eq!(text("1 KB to B").as_deref(), Some("1000 B"));
    assert_eq!(text("1 KiB to B").as_deref(), Some("1024 B"));
    assert_eq!(text("1 GiB to MiB").as_deref(), Some("1024 MiB"));
    assert_eq!(text("1 gb to mb").as_deref(), Some("1000 mb"));
    assert_eq!(text("1 B to b").as_deref(), Some("8 b"));
    assert_eq!(text("1 km to kg"), None, "cross-category");
}

#[test]
fn d017_bare_number_and_word_show_nothing() {
    for s in [
        "", "   ", "42", "-3.5", "(7)", "0xFF", "hello", "pi", "km", "5 km", "to hex",
    ] {
        assert_eq!(text(s), None, "{s:?}");
    }
}

#[test]
fn d017_division_by_zero_shows_nothing() {
    for s in ["1/0", "1 / (2 - 2)", "5 mod 0", "0/0", "0^-1"] {
        assert_eq!(text(s), None, "{s:?}");
    }
}

#[test]
fn d017_no_panic_on_hostile_input() {
    let long = |a: &str, n: usize| a.repeat(n);
    let table = [
        long("(", 1000),
        long(")", 1000),
        long("(1+", 300),
        long("-", 1000),
        long("2^", 500) + "2",
        long("9", 1000),
        format!("{}^{}", "9".repeat(500), "9".repeat(500)),
        "2^99999999999999999999".into(),
        format!("0x{}", "F".repeat(900)),
        "0b".into(),
        "0x".into(),
        "1e999".into(),
        "10 mod".into(),
        "% % %".into(),
        "5 km to".into(),
        "to to to".into(),
        "1.2.3 + 4".into(),
        "\u{1F600} + 1".into(),
        "é km to mi".into(),
        "1 ¹ to hex".into(),
        "99999999999999999999999999999999999999999 to hex".into(),
        "170141183460469231731687303715884105727 + 1 to hex".into(),
        "2^127 to hex".into(),
        "999999999999 km to mi".into(),
        "1 TiB to b".into(),
    ];
    for s in &table {
        let _ = evaluate(s); // must return, not panic or hang
    }
}
