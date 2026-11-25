use naobi_calc::{
    evaluate, evaluate_at, evaluate_currency, evaluate_with, parse_ans, CurrencyConversion,
    CurrencyOutcome, EvalOptions, FxRates, Kind, NumberFormat,
};

/// 2026-11-20T17:02:32Z in Unix milliseconds.
const NOW_2026_11_20_MS: i64 = 1_795_194_152_000;

fn text(input: &str) -> Option<String> {
    evaluate(input).map(|o| o.text)
}

fn text_at(input: &str, now_ms: i64) -> Option<String> {
    evaluate_at(input, now_ms).map(|o| o.text)
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
fn wcag_contrast_color_mix_and_everyday_math() {
    assert_eq!(
        text("contrast #ffffff #000000").as_deref(),
        Some("21.00:1 (AAA / AA)")
    );
    assert_eq!(
        text("contrast #fff #914a3b").as_deref(),
        Some("6.47:1 (AA)")
    );
    assert_eq!(text("mix #000 #fff 50%").as_deref(), Some("#808080"));
    assert_eq!(
        text("mix rgb(0, 0, 0) #ffffff 25%").as_deref(),
        Some("#404040")
    );
    assert_eq!(text("tip 15% on 80").as_deref(), Some("92 (+12)"));
    assert_eq!(text("vat 11% on 200").as_deref(), Some("222 (+22)"));
    assert_eq!(text("split 120 between 4").as_deref(), Some("30 each"));
    assert_eq!(text("0.75 to fraction").as_deref(), Some("3/4"));
    assert_eq!(text("round 3.14159 to 2 decimals").as_deref(), Some("3.14"));
    assert_eq!(text("ceil 2.1").as_deref(), Some("3"));
    assert_eq!(text("floor 2.9").as_deref(), Some("2"));
    assert_eq!(text("week 2026-11-25").as_deref(), Some("Week 48"));
    assert_eq!(
        text_at("time left", NOW_2026_11_20_MS).as_deref(),
        Some("6h 57m left today")
    );
    assert_eq!(
        text_at("days left in year", NOW_2026_11_20_MS).as_deref(),
        Some("41 days")
    );
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
        "epoch 999999999999999999".into(),
        "epoch -999999999999999999".into(),
        "9999-99-99T99:99:99Z".into(),
        "days until 99 dec".into(),
        "today + 999999999999 days".into(),
        "time in ".into(),
        "+5pm est in wib".into(),
    ];
    for s in &table {
        let _ = evaluate(s); // must return, not panic or hang
    }
}

#[test]
fn t4_epoch_and_iso8601_bidirectional() {
    // 10-digit seconds -> ISO-8601 UTC
    let sec = evaluate_at("1700000000", NOW_2026_11_20_MS).expect("10-digit epoch");
    assert_eq!(sec.text, "2023-11-14T22:13:20Z");
    assert_eq!(sec.kind, Kind::DateTime);

    // 13-digit milliseconds -> ISO-8601 UTC (omitting .000 when zero)
    assert_eq!(
        text_at("1700000000000", NOW_2026_11_20_MS).as_deref(),
        Some("2023-11-14T22:13:20Z")
    );
    assert_eq!(
        text_at("1700000000123", NOW_2026_11_20_MS).as_deref(),
        Some("2023-11-14T22:13:20.123Z")
    );

    // Explicit `epoch` / `unix` queries
    assert_eq!(
        text_at("epoch 0", NOW_2026_11_20_MS).as_deref(),
        Some("1970-01-01T00:00:00Z")
    );
    assert_eq!(
        text_at("epoch 42", NOW_2026_11_20_MS).as_deref(),
        Some("1970-01-01T00:00:42Z")
    );
    assert_eq!(
        text_at("unix -86400", NOW_2026_11_20_MS).as_deref(),
        Some("1969-12-31T00:00:00Z")
    );
    assert_eq!(
        text_at("epoch", NOW_2026_11_20_MS).as_deref(),
        Some("1795194152")
    );
    assert_eq!(
        text_at("epoch now", NOW_2026_11_20_MS).as_deref(),
        Some("1795194152")
    );
    assert_eq!(
        text_at("epoch ms", NOW_2026_11_20_MS).as_deref(),
        Some("1795194152000")
    );

    // ISO-8601 -> Unix epoch
    assert_eq!(
        text_at("2023-11-14T22:13:20Z", NOW_2026_11_20_MS).as_deref(),
        Some("1700000000")
    );
    assert_eq!(
        text_at("2023-11-14T22:13:20.123Z", NOW_2026_11_20_MS).as_deref(),
        Some("1700000000123")
    );
    assert_eq!(
        text_at("2023-11-15T05:13:20+07:00", NOW_2026_11_20_MS).as_deref(),
        Some("1700000000")
    );
    assert_eq!(
        text_at("2023-11-14", NOW_2026_11_20_MS).as_deref(),
        Some("1699920000")
    );
    assert_eq!(
        text_at("epoch 2023-11-14T22:13:20Z", NOW_2026_11_20_MS).as_deref(),
        Some("1700000000")
    );
    assert_eq!(
        text_at("2023-11-14T22:13:20Z to ms", NOW_2026_11_20_MS).as_deref(),
        Some("1700000000000")
    );

    // Guardrails: bare small/invalid numbers and arithmetic with spaces still behave per D-017
    assert_eq!(text_at("42", NOW_2026_11_20_MS), None);
    assert_eq!(text_at("0000000000", NOW_2026_11_20_MS), None);
    assert_eq!(text_at("12345678", NOW_2026_11_20_MS), None);
    assert_eq!(text_at("2026-02-29", NOW_2026_11_20_MS), None);
    assert_eq!(
        text_at("2023 - 11 - 14", NOW_2026_11_20_MS).as_deref(),
        Some("1998")
    );
    assert_eq!(
        text_at("1700000000 + 1", NOW_2026_11_20_MS).as_deref(),
        Some("1700000001")
    );
}

#[test]
fn t4_date_arithmetic() {
    // `NOW_2026_11_20_MS` is 2026-11-20.
    assert_eq!(
        text_at("days until 31 dec", NOW_2026_11_20_MS).as_deref(),
        Some("41 days")
    );
    assert_eq!(
        text_at("days until 2026-11-21", NOW_2026_11_20_MS).as_deref(),
        Some("1 day")
    );
    assert_eq!(
        text_at("days until 2026-11-20", NOW_2026_11_20_MS).as_deref(),
        Some("0 days")
    );
    // Month/day without year rolls forward to next year if already passed in 2026.
    assert_eq!(
        text_at("days until 1 jan", NOW_2026_11_20_MS).as_deref(),
        Some("42 days")
    );
    // Feb 29 without year rolls forward to next leap year (2028).
    assert_eq!(
        text_at("days until 29 feb", NOW_2026_11_20_MS).as_deref(),
        Some("466 days")
    );
    // `days since`
    assert_eq!(
        text_at("days since 1 jan 2025", NOW_2026_11_20_MS).as_deref(),
        Some("688 days")
    );
    assert_eq!(
        text_at("days since 19 nov", NOW_2026_11_20_MS).as_deref(),
        Some("1 day")
    );
    // `days between`
    assert_eq!(
        text_at("days between 2026-01-01 and 2026-01-31", NOW_2026_11_20_MS).as_deref(),
        Some("30 days")
    );
    // `<date> +/- <N> <unit>`
    assert_eq!(
        text_at("today + 14 days", NOW_2026_11_20_MS).as_deref(),
        Some("2026-12-04")
    );
    assert_eq!(
        text_at("2026-12-31 - 10 days", NOW_2026_11_20_MS).as_deref(),
        Some("2026-12-21")
    );
    assert_eq!(
        text_at("tomorrow + 1 week", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-28")
    );
    assert_eq!(
        text_at("yesterday - 2 days", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-17")
    );
    assert_eq!(
        text_at("2026-01-31 + 1 month", NOW_2026_11_20_MS).as_deref(),
        Some("2026-02-28")
    );
    assert_eq!(
        text_at("2024-02-29 + 1 year", NOW_2026_11_20_MS).as_deref(),
        Some("2025-02-28")
    );
    // `<date1> - <date2>`
    assert_eq!(
        text_at("2026-12-31 - 2026-11-20", NOW_2026_11_20_MS).as_deref(),
        Some("41 days")
    );
    assert_eq!(
        evaluate_at("today + 14 days", NOW_2026_11_20_MS).map(|o| o.kind),
        Some(Kind::DateTime)
    );
}

#[test]
fn t4_timezone_and_dst_conversions() {
    // 2026-07-15T12:00:00Z (Northern summer: US EDT, EU CEST/BST, AU AEST)
    const SUMMER_2026_07_15_MS: i64 = 1_784_116_800_000;
    // 2026-03-15T12:00:00Z (US already on EDT since Mar 8; EU still on GMT/CET until Mar 29)
    const MARCH_GAP_2026_03_15_MS: i64 = 1_773_576_000_000;

    // Fixed abbreviation conversions
    assert_eq!(
        text_at("5pm est in wib", NOW_2026_11_20_MS).as_deref(),
        Some("05:00 WIB (+1d, UTC+7)")
    );
    assert_eq!(
        text_at("3pm utc in jst", NOW_2026_11_20_MS).as_deref(),
        Some("00:00 JST (+1d, UTC+9)")
    );
    assert_eq!(
        text_at("9am utc in jst", NOW_2026_11_20_MS).as_deref(),
        Some("18:00 JST (UTC+9)")
    );
    assert_eq!(
        text_at("12am pst to ist", NOW_2026_11_20_MS).as_deref(),
        Some("13:30 IST (UTC+5:30)")
    );
    assert_eq!(
        text_at("12pm gmt in sgt", NOW_2026_11_20_MS).as_deref(),
        Some("20:00 SGT (UTC+8)")
    );
    assert_eq!(
        text_at("14:30 cet in wita", NOW_2026_11_20_MS).as_deref(),
        Some("21:30 WITA (UTC+8)")
    );
    assert_eq!(
        text_at("9:00 wit to kst", NOW_2026_11_20_MS).as_deref(),
        Some("09:00 KST (UTC+9)")
    );
    assert_eq!(
        text_at("9am jst in pst", NOW_2026_11_20_MS).as_deref(),
        Some("16:00 PST (-1d, UTC-8)")
    );
    assert_eq!(
        text_at("10am aest in bst", NOW_2026_11_20_MS).as_deref(),
        Some("01:00 BST (UTC+1)")
    );
    assert_eq!(
        text_at("5pm pdt in wib", NOW_2026_11_20_MS).as_deref(),
        Some("07:00 WIB (+1d, UTC+7)")
    );
    assert_eq!(
        text_at("10:00 utc+2 in utc+7", NOW_2026_11_20_MS).as_deref(),
        Some("15:00 UTC+7 (UTC+7)")
    );

    // `time in <zone>` / `now in <zone>` / `<zone> time` with DST awareness
    assert_eq!(
        text_at("time in tokyo", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-21 02:02 JST (UTC+9)")
    );
    assert_eq!(
        text_at("time in london", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-20 17:02 GMT (UTC+0)")
    );
    assert_eq!(
        text_at("time in london", SUMMER_2026_07_15_MS).as_deref(),
        Some("2026-07-15 13:00 BST (UTC+1)")
    );
    assert_eq!(
        text_at("time in new york", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-20 12:02 EST (UTC-5)")
    );
    assert_eq!(
        text_at("time in new york", SUMMER_2026_07_15_MS).as_deref(),
        Some("2026-07-15 08:00 EDT (UTC-4)")
    );
    assert_eq!(
        text_at("time in sydney", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-21 04:02 AEDT (UTC+11)")
    );
    assert_eq!(
        text_at("time in sydney", SUMMER_2026_07_15_MS).as_deref(),
        Some("2026-07-15 22:00 AEST (UTC+10)")
    );
    assert_eq!(
        text_at("now in jakarta", NOW_2026_11_20_MS).as_deref(),
        Some("2026-11-21 00:02 WIB (UTC+7)")
    );
    assert_eq!(
        text_at("singapore time", SUMMER_2026_07_15_MS).as_deref(),
        Some("2026-07-15 20:00 SGT (UTC+8)")
    );

    // City-to-city DST divergence during US-EU March gap
    assert_eq!(
        text_at("5pm new york in london", NOW_2026_11_20_MS).as_deref(),
        Some("22:00 GMT (UTC+0)")
    );
    assert_eq!(
        text_at("5pm new york in london", MARCH_GAP_2026_03_15_MS).as_deref(),
        Some("21:00 GMT (UTC+0)")
    );
    assert_eq!(
        text_at("5pm new york in london", SUMMER_2026_07_15_MS).as_deref(),
        Some("22:00 BST (UTC+1)")
    );

    // Exact EU spring-forward boundary: 2026-03-29T00:59Z (GMT) -> 01:00Z (BST)
    assert_eq!(
        text_at("time in london", 1_774_745_940_000).as_deref(),
        Some("2026-03-29 00:59 GMT (UTC+0)")
    );
    assert_eq!(
        text_at("time in london", 1_774_746_000_000).as_deref(),
        Some("2026-03-29 02:00 BST (UTC+1)")
    );

    assert_eq!(
        evaluate_at("time in tokyo", NOW_2026_11_20_MS).map(|o| o.kind),
        Some(Kind::DateTime)
    );
}

#[test]
fn currency_symbols_strip_cleanly() {
    assert_eq!(text("$120 + $45").as_deref(), Some("165"));
    assert_eq!(text("€19.99 + €0.01").as_deref(), Some("20"));
    assert_eq!(text("£100 - £25%").as_deref(), Some("75"));
    assert_eq!(text("¥1000 / 4").as_deref(), Some("250"));
    assert_eq!(text("Rp50000 * 2").as_deref(), Some("100000"));
    assert_eq!(text("Rp 50000 + Rp 25000").as_deref(), Some("75000"));
    assert_eq!(text("-$50 + $100").as_deref(), Some("50"));
    assert_eq!(text("($10 + $20) * 2").as_deref(), Some("60"));
    assert_eq!(text("$100 to hex").as_deref(), Some("0x64"));
    // Bare currency numbers and malformed symbols still show nothing.
    for s in [
        "$120", "Rp50000", "$", "Rp", "$$10 + 1", "Rp + 5", "rpn + 1",
    ] {
        assert_eq!(text(s), None, "{s:?}");
    }
}

#[test]
fn ans_reuses_previous_result_and_parses_outcomes() {
    let with_ans = |input: &str, ans: Option<f64>| {
        evaluate_with(
            input,
            EvalOptions {
                ans,
                ..EvalOptions::default()
            },
        )
        .map(|o| o.text)
    };
    assert_eq!(with_ans("ans + 10", Some(5.0)).as_deref(), Some("15"));
    assert_eq!(with_ans("ANS * 2", Some(5.0)).as_deref(), Some("10"));
    assert_eq!(with_ans("ans%", Some(25.0)).as_deref(), Some("0.25"));
    assert_eq!(with_ans("200 + ans%", Some(10.0)).as_deref(), Some("220"));
    assert_eq!(with_ans("ans to hex", Some(255.0)).as_deref(), Some("0xFF"));
    assert_eq!(
        with_ans("ans km to mi", Some(5.0)).as_deref(),
        Some("3.106855961 mi")
    );
    // Bare ans and non-word prefixes must not hijack search.
    assert_eq!(with_ans("ans", Some(5.0)), None);
    assert_eq!(with_ans("answer + 1", Some(5.0)), None);
    assert_eq!(with_ans("ansi + 1", Some(5.0)), None);
    assert_eq!(with_ans("ans + 1", None), None);
    assert_eq!(with_ans("ans + 1", Some(f64::NAN)), None);

    assert_eq!(parse_ans("1177.6"), Some(1177.6));
    assert_eq!(parse_ans("0xFF"), Some(255.0));
    assert_eq!(parse_ans("-0xFF"), Some(-255.0));
    assert_eq!(parse_ans("0b1010"), Some(10.0));
    assert_eq!(parse_ans("3.106855961 mi"), Some(3.106855961));
    assert_eq!(parse_ans("bad"), None);
}

#[test]
fn css_px_and_rem_convert_using_configurable_base() {
    assert_eq!(text("32 px to rem").as_deref(), Some("2 rem"));
    assert_eq!(text("1.5 rem to px").as_deref(), Some("24 px"));
    assert_eq!(text("16px to rem").as_deref(), Some("1 rem"));
    assert_eq!(text("2rem in px").as_deref(), Some("32 px"));
    assert_eq!(text("24 PX TO REM").as_deref(), Some("1.5 REM"));
    assert_eq!(text("16 px to px").as_deref(), Some("16 px"));
    assert_eq!(
        evaluate("32 px to rem").map(|o| o.kind),
        Some(Kind::Conversion)
    );
    assert_eq!(text("16 px to km"), None, "cross-category");
    assert_eq!(text("1 rem to m"), None, "cross-category");

    let custom = |input: &str, rem_base_px: f64, ans: Option<f64>| {
        evaluate_with(input, EvalOptions { ans, rem_base_px }).map(|o| o.text)
    };
    assert_eq!(
        custom("25 px to rem", 10.0, None).as_deref(),
        Some("2.5 rem")
    );
    assert_eq!(
        custom("2.5 rem to px", 10.0, None).as_deref(),
        Some("25 px")
    );
    assert_eq!(
        custom("ans px to rem", 16.0, Some(32.0)).as_deref(),
        Some("2 rem")
    );
    assert_eq!(custom("16 px to rem", 0.0, None), None);
    assert_eq!(custom("16 px to rem", -16.0, None), None);
    assert_eq!(custom("16 px to rem", f64::NAN, None), None);
}

#[test]
fn currency_conversion_triangulates_formats_and_detects_missing_cache() {
    let json = r#"{
        "amount": 1.0,
        "base": "EUR",
        "date": "2026-11-19",
        "rates": {
            "USD": 1.05,
            "IDR": 19065.9,
            "JPY": 162.75,
            "GBP": 0.84
        }
    }"#;
    let rates = FxRates::from_json(json).expect("valid fx json");
    assert_eq!(rates.base, "EUR");
    assert_eq!(rates.date, "2026-11-19");
    assert_eq!(FxRates::from_json("{}"), None);
    assert_eq!(
        FxRates::from_json(r#"{"base":"EUR","date":"2026-11-19","rates":{"USD":-1.0}}"#),
        None
    );

    let conv = |q: &str, default: &str, fmt: NumberFormat| -> CurrencyConversion {
        match evaluate_currency(q, default, fmt, Some(&rates)) {
            Some(CurrencyOutcome::Converted(c)) => c,
            other => panic!("expected Converted for {q:?}, got {other:?}"),
        }
    };

    // Bare currency amounts -> default_currency ("IDR")
    let c = conv("53 usd", "IDR", NumberFormat::Us);
    assert_eq!(c.display, "962,374.00 IDR");
    assert_eq!(c.raw, "962374.00");
    assert_eq!(c.subtitle, "53 USD to IDR · Rates 2026-11-19");

    let sym = conv("$53", "IDR", NumberFormat::Us);
    assert_eq!(sym.display, "962,374.00 IDR");
    assert_eq!(sym.raw, "962374.00");
    assert_eq!(sym.subtitle, "53 USD to IDR · Rates 2026-11-19");

    let k = conv("53k usd", "IDR", NumberFormat::Us);
    assert_eq!(k.display, "962,374,000.00 IDR");
    assert_eq!(k.raw, "962374000.00");

    let m = conv("1.5m usd", "IDR", NumberFormat::Us);
    assert_eq!(m.display, "27,237,000,000.00 IDR");
    assert_eq!(m.raw, "27237000000.00");

    // Base currency ("EUR") has implicit rate 1.0
    let eur = conv("€10", "IDR", NumberFormat::Us);
    assert_eq!(eur.display, "190,659.00 IDR");
    assert_eq!(eur.subtitle, "10 EUR to IDR · Rates 2026-11-19");

    // Explicit pairs
    let pair = conv("53 usd to eur", "IDR", NumberFormat::Us);
    assert_eq!(pair.display, "50.48 EUR");
    assert_eq!(pair.raw, "50.48");
    assert_eq!(pair.subtitle, "53 USD to EUR · Rates 2026-11-19");

    let jpy = conv("53k usd in jpy", "IDR", NumberFormat::Us);
    assert_eq!(jpy.display, "8,215,000.00 JPY");
    assert_eq!(jpy.raw, "8215000.00");
    assert_eq!(jpy.subtitle, "53000 USD to JPY · Rates 2026-11-19");

    let rp = conv("Rp 19065.9 to usd", "IDR", NumberFormat::Us);
    assert_eq!(rp.display, "1.05 USD");
    assert_eq!(rp.raw, "1.05");

    // Indonesian number format ("id")
    let id_idr = conv("53 usd", "IDR", NumberFormat::Id);
    assert_eq!(id_idr.display, "962.374,00 IDR");
    assert_eq!(id_idr.raw, "962374.00");

    let id_eur = conv("53 usd to eur", "IDR", NumberFormat::Id);
    assert_eq!(id_eur.display, "50,48 EUR");
    assert_eq!(id_eur.raw, "50.48");

    // Missing cache returns MissingRates only for currency queries
    for q in ["53 usd", "$53", "53k usd", "53 usd to eur"] {
        assert!(
            matches!(
                evaluate_currency(q, "IDR", NumberFormat::Us, None),
                Some(CurrencyOutcome::MissingRates { .. })
            ),
            "{q:?}"
        );
    }
    for non_currency in ["42", "hello", "5 km to mi", "$120 + $45", "53 xyz", "53k"] {
        assert_eq!(
            evaluate_currency(non_currency, "IDR", NumberFormat::Us, None),
            None,
            "{non_currency:?}"
        );
        assert_eq!(
            evaluate_currency(non_currency, "IDR", NumberFormat::Us, Some(&rates)),
            None,
            "{non_currency:?}"
        );
    }
}

#[test]
fn fx_rates_parses_ecb_xml_and_roundtrips_cache_json() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
  <gesmes:subject>Reference rates</gesmes:subject>
  <gesmes:Sender><gesmes:name>European Central Bank</gesmes:name></gesmes:Sender>
  <Cube>
    <Cube time='2026-11-20'>
      <Cube currency='USD' rate='1.05'/>
      <Cube currency="JPY" rate="162.75" />
      <Cube currency='IDR' rate='19065.9'/>
    </Cube>
  </Cube>
</gesmes:Envelope>"#;

    let parsed = FxRates::from_ecb_xml(xml).expect("valid ECB XML");
    assert_eq!(parsed.base, "EUR");
    assert_eq!(parsed.date, "2026-11-20");
    assert_eq!(parsed.rate("USD"), Some(1.05));
    assert_eq!(parsed.rate("IDR"), Some(19065.9));
    assert_eq!(parsed.rate("EUR"), Some(1.0));

    let json = parsed.to_json();
    let roundtrip = FxRates::from_json(&json).expect("roundtrip JSON");
    assert_eq!(roundtrip, parsed);

    match evaluate_currency("53 usd", "IDR", NumberFormat::Us, Some(&roundtrip)) {
        Some(CurrencyOutcome::Converted(c)) => {
            assert_eq!(c.display, "962,374.00 IDR");
            assert_eq!(c.subtitle, "53 USD to IDR · Rates 2026-11-20");
        }
        other => panic!("expected Converted, got {other:?}"),
    }

    // Malformed, missing date, empty rates, negative/zero/NaN rates, bad currency codes all fail cleanly.
    for bad in [
        "",
        "<Cube></Cube>",
        "<Cube><Cube currency='USD' rate='1.05'/></Cube>",
        "<Cube><Cube time='2026-11-20'></Cube></Cube>",
        "<Cube><Cube time='2026-11-20'><Cube currency='USD' rate='-1.0'/></Cube></Cube>",
        "<Cube><Cube time='2026-11-20'><Cube currency='USD' rate='0'/></Cube></Cube>",
        "<Cube><Cube time='2026-11-20'><Cube currency='USD' rate='NaN'/></Cube></Cube>",
        "<Cube><Cube time='2026-11-20'><Cube currency='US' rate='1.05'/></Cube></Cube>",
        "<Cube><Cube time='2026-11-20'><Cube currency='USD' rate='1.05'",
    ] {
        assert_eq!(FxRates::from_ecb_xml(bad), None, "{bad:?}");
    }
}
