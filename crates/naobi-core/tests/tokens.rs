//! Token contrast (R-007): every text and component pair in DESIGN_SYSTEM.md meets WCAG 2.2 AA.
use naobi_core::tokens::{Palette, Rgba, PAPER, STONE};

fn luminance(c: Rgba) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Composites `fg` (with its alpha) over an opaque `bg`.
fn over(fg: Rgba, bg: Rgba) -> Rgba {
    let mix = |f: u8, b: u8| {
        let a = f64::from(fg.a) / 255.0;
        (f64::from(f) * a + f64::from(b) * (1.0 - a)).round() as u8
    };
    Rgba {
        r: mix(fg.r, bg.r),
        g: mix(fg.g, bg.g),
        b: mix(fg.b, bg.b),
        a: 255,
    }
}

/// (name, foreground, background, minimum ratio). Text needs 4.5:1, UI components and the
/// accent or focus marks 3:1.
fn pairs(p: &Palette) -> Vec<(&'static str, Rgba, Rgba, f64)> {
    let text = 4.5;
    let ui = 3.0;
    vec![
        ("primary on window", p.text_primary, p.window, text),
        ("primary on raised", p.text_primary, p.raised, text),
        ("primary on selected", p.text_primary, p.selected, text),
        ("primary on well", p.text_primary, p.well, text),
        ("secondary on window", p.text_secondary, p.window, text),
        // Subtitles, hint chips and action reasons on a selected row all use this pair. Muted ink
        // is not listed here: it fails on the selected wash (see the r007 test below).
        (
            "subtitle on selected",
            p.subtitle_selected,
            p.selected,
            text,
        ),
        (
            "hint chip on selected",
            p.subtitle_selected,
            p.selected,
            text,
        ),
        (
            "action reason on selected",
            p.subtitle_selected,
            p.selected,
            text,
        ),
        ("muted on window", p.text_muted, p.window, text),
        ("muted on raised", p.text_muted, p.raised, text),
        ("muted on well", p.text_muted, p.well, text),
        ("danger on window", p.danger, p.window, text),
        ("danger on selected", p.danger, p.selected, text),
        ("success on window", p.success, p.window, text),
        ("accent on window", p.accent, p.window, ui),
        ("accent on selected", p.accent, p.selected, ui),
        ("focus on window", p.focus, p.window, ui),
        ("focus on raised", p.focus, p.raised, ui),
        ("focus on selected", p.focus, p.selected, ui),
    ]
}

// @R-007
#[test]
fn r007_token_pairs_meet_wcag_aa() {
    for (theme, p) in [("paper", PAPER), ("stone", STONE)] {
        for (name, fg, bg, min) in pairs(&p) {
            let ratio = contrast(over(fg, bg), bg);
            assert!(
                ratio >= min,
                "{theme} {name}: {ratio:.2}:1 is below {min}:1"
            );
        }
    }
}

// @R-007
#[test]
fn r007_muted_on_selected_paper_row_is_replaced_by_secondary() {
    // The one failing pair in the mockup: ink-muted on a selected paper row.
    assert!(contrast(PAPER.text_muted, PAPER.selected) < 4.5);
    assert_eq!(PAPER.subtitle_selected, PAPER.text_secondary);
    assert!(contrast(PAPER.subtitle_selected, PAPER.selected) >= 4.5);
}
