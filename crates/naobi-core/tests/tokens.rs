//! Token contrast (R-007): every text and component pair in DESIGN_SYSTEM.md meets WCAG 2.2 AA.
use naobi_core::tokens::{contrast, contrast_failures, PAPER, STONE};

// @R-007
#[test]
fn r007_token_pairs_meet_wcag_aa() {
    for (theme, p) in [("paper", PAPER), ("stone", STONE)] {
        assert_eq!(contrast_failures(&p), [], "{theme}");
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
