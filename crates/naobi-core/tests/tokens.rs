//! Token contrast (R-007): every text and component pair in DESIGN_SYSTEM.md meets WCAG 2.2 AA.
use naobi_core::theme_file::builtins;
use naobi_core::tokens::{contrast, contrast_failures, Rgba, PAPER};

// @R-007
#[test]
fn r007_token_pairs_meet_wcag_aa() {
    for t in builtins() {
        assert_eq!(contrast_failures(&t.palette), [], "{}", t.id);
    }
}

// @R-007
#[test]
fn r007_builtin_windows_and_accents_are_distinct() {
    let all = builtins();
    let dist = |a: Rgba, b: Rgba| {
        let d = |x: u8, y: u8| (f64::from(x) - f64::from(y)).powi(2);
        (d(a.r, b.r) + d(a.g, b.g) + d(a.b, b.b)).sqrt()
    };
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert!(
                dist(a.palette.window, b.palette.window) >= 12.0,
                "window {} {}",
                a.id,
                b.id
            );
            assert!(
                dist(a.palette.accent, b.palette.accent) >= 30.0,
                "accent {} {}",
                a.id,
                b.id
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

// @R-007
#[test]
fn r007_contrast_matches_the_wcag_reference_values() {
    let close = |a: f64, b: f64| (a - b).abs() < 0.005;
    let (black, white) = (Rgba::rgb(0x000000), Rgba::rgb(0xFFFFFF));
    assert!(close(contrast(black, white), 21.0));
    assert!(close(contrast(white, black), 21.0));
    // Pure red alone weighs 0.2126; a near-black grey sits on the linear part of the curve.
    assert!(close(contrast(Rgba::rgb(0xFF0000), black), 5.252));
    assert!(close(contrast(Rgba::rgb(0x0A0A0A), black), 1.0607));
    assert!(close(contrast(Rgba::rgb(0x777777), white), 4.478));
}

// @R-033
#[test]
fn r033_half_transparent_ink_is_mixed_over_its_background() {
    use naobi_core::tokens::over;
    let half_red = Rgba::with_alpha(0xFF0000, 128);
    assert_eq!(over(half_red, Rgba::rgb(0x000000)), Rgba::rgb(0x800000));
    assert_eq!(
        over(Rgba::with_alpha(0x000000, 128), Rgba::rgb(0xFFFFFF)),
        Rgba::rgb(0x7F7F7F)
    );
}
