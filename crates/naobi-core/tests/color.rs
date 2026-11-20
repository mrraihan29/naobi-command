use naobi_core::color::{format, parse, Format, ParsedColor, Rgb};

#[test]
fn parses_hex_3_and_6_digits_and_formats_all_notations() {
    let parsed = parse("  #914a3b  ").expect("valid 6-digit hex");
    assert_eq!(
        parsed,
        ParsedColor {
            rgb: Rgb {
                r: 0x91,
                g: 0x4A,
                b: 0x3B,
            },
            format: Format::Hex,
        }
    );
    assert_eq!(format(parsed.rgb, Format::Hex), "#914A3B");
    assert_eq!(format(parsed.rgb, Format::Rgb), "rgb(145, 74, 59)");
    assert_eq!(format(parsed.rgb, Format::Hsl), "hsl(10, 42%, 40%)");
    assert_eq!(format(parsed.rgb, Format::Oklch), "oklch(49.3% 0.099 33.1)");

    let short = parse("#f0a").expect("valid 3-digit hex");
    assert_eq!(
        short,
        ParsedColor {
            rgb: Rgb {
                r: 0xFF,
                g: 0x00,
                b: 0xAA,
            },
            format: Format::Hex,
        }
    );
}

#[test]
fn parses_rgb_and_rejects_invalid_inputs() {
    assert_eq!(
        parse("rgb(145, 74, 59)"),
        Some(ParsedColor {
            rgb: Rgb {
                r: 145,
                g: 74,
                b: 59,
            },
            format: Format::Rgb,
        })
    );
    assert_eq!(
        parse("RGB( 255 0 128 )"),
        Some(ParsedColor {
            rgb: Rgb {
                r: 255,
                g: 0,
                b: 128,
            },
            format: Format::Rgb,
        })
    );

    for bad in [
        "",
        "914A3B",
        "#",
        "#12",
        "#1234",
        "#12345",
        "#1234567",
        "#GGGGGG",
        "#+12",
        "rgb()",
        "rgb(1, 2)",
        "rgb(1, 2, 3, 4)",
        "rgb(256, 0, 0)",
        "rgb(-1, 0, 0)",
        "rgb(1.5, 0, 0)",
        "rgb(1, 2, 3",
        "rgb(1,,2,3)",
        "🔥",
        "🔥()",
    ] {
        assert_eq!(parse(bad), None, "expected None for {bad:?}");
    }
}

#[test]
fn parses_hsl_round_trip_and_rejects_invalid() {
    for (input, want) in [
        ("hsl(0, 100%, 50%)", Rgb { r: 255, g: 0, b: 0 }),
        ("hsl(120, 100%, 50%)", Rgb { r: 0, g: 255, b: 0 }),
        ("hsl(240, 100%, 50%)", Rgb { r: 0, g: 0, b: 255 }),
        ("hsl(0, 0%, 0%)", Rgb { r: 0, g: 0, b: 0 }),
        (
            "hsl(0, 0%, 100%)",
            Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
        ),
        (
            "HSL(148deg 90% 41%)",
            Rgb {
                r: 10,
                g: 199,
                b: 98,
            },
        ),
    ] {
        let parsed = parse(input).expect("valid hsl");
        assert_eq!(parsed.format, Format::Hsl);
        assert_eq!(parsed.rgb, want, "for {input}");
    }

    // Round-trip formatted HSL from RGB stays within rounding tolerance (1 LSB) and formats identically.
    for rgb in [
        Rgb { r: 255, g: 0, b: 0 },
        Rgb { r: 0, g: 255, b: 0 },
        Rgb { r: 0, g: 0, b: 255 },
        Rgb { r: 0, g: 0, b: 0 },
        Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Rgb {
            r: 0x91,
            g: 0x4A,
            b: 0x3B,
        },
    ] {
        let s = format(rgb, Format::Hsl);
        let back = parse(&s).expect("round-trip hsl");
        assert_eq!(
            format(back.rgb, Format::Hsl),
            s,
            "round-trip {rgb:?} -> {s}"
        );
    }

    for bad in [
        "hsl()",
        "hsl(0, 100%)",
        "hsl(361, 50%, 50%)",
        "hsl(-10, 50%, 50%)",
        "hsl(0, 101%, 50%)",
        "hsl(0, 50%, -1%)",
        "hsl(0, 50, 50)",
        "hsl(NaN, 50%, 50%)",
    ] {
        assert_eq!(parse(bad), None, "expected None for {bad:?}");
    }
}

#[test]
fn parses_oklch_round_trip_and_rejects_invalid() {
    assert_eq!(
        parse("oklch(0.0% 0.000 0.0)"),
        Some(ParsedColor {
            rgb: Rgb { r: 0, g: 0, b: 0 },
            format: Format::Oklch,
        })
    );
    assert_eq!(
        parse("OKLCH(100.0% 0.000 0.0)"),
        Some(ParsedColor {
            rgb: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            format: Format::Oklch,
        })
    );
    let unit = parse("oklch(0.493 0.099 33.1deg)").expect("valid unit lightness oklch");
    assert_eq!(unit.format, Format::Oklch);
    assert_eq!(
        unit.rgb,
        Rgb {
            r: 0x91,
            g: 0x4A,
            b: 0x3B,
        }
    );

    // Round-trip formatted OKLCH from RGB matches within 1 LSB and formats closely.
    for rgb in [
        Rgb { r: 255, g: 0, b: 0 },
        Rgb { r: 0, g: 255, b: 0 },
        Rgb { r: 0, g: 0, b: 255 },
        Rgb {
            r: 0x91,
            g: 0x4A,
            b: 0x3B,
        },
        Rgb {
            r: 10,
            g: 200,
            b: 100,
        },
        Rgb {
            r: 20,
            g: 40,
            b: 200,
        },
        Rgb { r: 3, g: 10, b: 7 },
    ] {
        let s = format(rgb, Format::Oklch);
        let back = parse(&s).expect("round-trip oklch");
        assert!(
            back.rgb.r.abs_diff(rgb.r) <= 2
                && back.rgb.g.abs_diff(rgb.g) <= 2
                && back.rgb.b.abs_diff(rgb.b) <= 2,
            "round-trip {rgb:?} via {s} gave {:?}",
            back.rgb
        );
    }

    for bad in [
        "oklch()",
        "oklch(50% 0.1)",
        "oklch(101% 0.1 0)",
        "oklch(-1% 0.1 0)",
        "oklch(1.5 0.1 0)",
        "oklch(50% -0.1 0)",
        "oklch(50% 5.0 0)",
        "oklch(50% 0.1 400)",
        "oklch(50% 0.1 -1)",
        "oklch(NaN 0.1 0)",
    ] {
        assert_eq!(parse(bad), None, "expected None for {bad:?}");
    }
}
