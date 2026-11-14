//! Drawn icons for the built-in command rows (NCMD-29): our own simple shapes on a 24 x 24 grid, as SVG
//! path data. Each glyph is an outline (stroked) and an optional region (filled, drawn faint): the snap
//! commands show which part of the screen they take. No file, no font, no download.

/// `(item id, outline, region)`.
const GLYPHS: &[(&str, &str, &str)] = &[
    // System commands.
    ("sys.lock", "M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3", ""),
    ("sys.sleep", "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z", ""),
    ("sys.hibernate", "M12 3v18M4.5 7.5l15 9M19.5 7.5l-15 9", ""),
    ("sys.restart", "M20 12a8 8 0 1 1-2.5-5.8M20 4v5h-5", ""),
    ("sys.shut_down", "M12 3v9M6.5 6.5a8 8 0 1 0 11 0", ""),
    ("sys.sign_out", "M10 4H5v16h5M14 8l4 4-4 4M18 12H9", ""),
    ("sys.empty_recycle_bin", "M5 7h14M9 7V4h6v3M7 7l1 13h8l1-13M10 11v6M14 11v6", ""),
    ("sys.toggle_dark_mode", "M12 3a9 9 0 1 0 0 18a9 9 0 0 0 0-18z", "M12 3a9 9 0 0 0 0 18z"),
    (
        "sys.open_windows_settings",
        "M12 9a3 3 0 1 0 0 6a3 3 0 0 0 0-6zM12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1",
        "",
    ),
    ("sys.open_settings_file", "M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6", ""),
    ("sys.pause_hotkeys", "M8 5v14M16 5v14", ""),
    ("sys.quit", "M6 6l12 12M18 6L6 18", ""),
    // Window management: the screen, and the part the command takes.
    ("win.snap_left", "M3 5h18v14H3z", "M3 5h9v14H3z"),
    ("win.snap_right", "M3 5h18v14H3z", "M12 5h9v14h-9z"),
    ("win.snap_up", "M3 5h18v14H3z", "M3 5h18v7H3z"),
    ("win.snap_down", "M3 5h18v14H3z", "M3 12h18v7H3z"),
    ("win.snap_top_left", "M3 5h18v14H3z", "M3 5h9v7H3z"),
    ("win.snap_top_right", "M3 5h18v14H3z", "M12 5h9v7h-9z"),
    ("win.snap_bottom_left", "M3 5h18v14H3z", "M3 12h9v7H3z"),
    ("win.snap_bottom_right", "M3 5h18v14H3z", "M12 12h9v7h-9z"),
    ("win.snap_left_third", "M3 5h18v14H3z", "M3 5h6v14H3z"),
    ("win.snap_center_third", "M3 5h18v14H3z", "M9 5h6v14H9z"),
    ("win.snap_right_third", "M3 5h18v14H3z", "M15 5h6v14h-6z"),
    ("win.snap_first_two_thirds", "M3 5h18v14H3z", "M3 5h12v14H3z"),
    ("win.snap_last_two_thirds", "M3 5h18v14H3z", "M9 5h12v14H9z"),
    ("win.center", "M3 5h18v14H3z", "M7 8h10v8H7z"),
    ("win.maximize", "M3 5h18v14H3z", "M3 5h18v14H3z"),
    ("win.almost_maximize", "M3 5h18v14H3z", "M5 7h14v10H5z"),
    ("win.restore", "M8 9h11v10H8zM11 5h10v10", ""),
    ("win.next_display", "M2 6h11v9H2zM15 9h7v8h-7zM5 19h5", ""),
    // Always on Top, Awake, color picker.
    ("aot.toggle", "M9 4h6l-1 6 3 3H7l3-3zM12 13v7", ""),
    ("awake.on", COFFEE, ""),
    ("awake.off", COFFEE, ""),
    ("awake.for.30", COFFEE, ""),
    ("awake.for.60", COFFEE, ""),
    ("awake.for.120", COFFEE, ""),
    ("awake.display.on", "M3 5h18v11H3zM9 20h6M12 16v4", ""),
    ("awake.display.off", "M3 5h18v11H3zM9 20h6M12 16v4M4 4l16 16", ""),
    ("picker.pick", "M15 4l5 5-3 3-5-5zM12 7l-7 7v5h5l7-7", ""),
    ("picker.history", "M12 3a9 9 0 1 0 0 18a9 9 0 0 0 0-18zM12 7v5l3 2", ""),
    // Rows that are not commands of a module.
    ("quicklink", "M12 3a9 9 0 1 0 0 18a9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18", ""),
    ("calc", "M5 9h14M5 15h14", ""),
];

const COFFEE: &str =
    "M5 9h11v6a4 4 0 0 1-4 4H9a4 4 0 0 1-4-4zM16 11h2a2 2 0 0 1 0 4h-2M8 3v3M12 3v3";

/// Any other command: a prompt chevron.
const GENERIC: (&str, &str) = ("M5 7l5 5-5 5M12 17h7", "");

/// The outline and the region of the glyph for an item id, or the generic command glyph.
pub fn glyph(id: &str) -> (&'static str, &'static str) {
    GLYPHS
        .iter()
        .find(|(i, _, _)| *i == id)
        .map_or(GENERIC, |(_, outline, region)| (outline, region))
}

/// True when `id` has a glyph of its own, not the generic one.
pub fn has_own_glyph(id: &str) -> bool {
    GLYPHS.iter().any(|(i, _, _)| *i == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-007
    #[test]
    fn r007_every_glyph_is_distinct_path_data_and_unknown_ids_get_the_generic_one() {
        for (id, outline, region) in GLYPHS {
            assert!(outline.starts_with('M'), "{id}");
            assert!(region.is_empty() || region.starts_with('M'), "{id}");
            // Path data is letters, digits, signs, dots, commas and spaces only.
            assert!(
                outline
                    .chars()
                    .chain(region.chars())
                    .all(|c| c.is_ascii_alphanumeric() || "-. ,".contains(c)),
                "{id}"
            );
        }
        let mut seen = std::collections::HashSet::new();
        for (id, o, r) in GLYPHS {
            // The Awake timers and on/off share the cup on purpose; every other glyph is its own.
            if !id.starts_with("awake.") || id.contains("display") {
                assert!(seen.insert((o, r)), "{id} repeats another glyph");
            }
        }
        assert_eq!(glyph("sys.lock").0, GLYPHS[0].1);
        assert_eq!(glyph("module.unknown"), GENERIC);
        assert!(has_own_glyph("win.snap_left") && !has_own_glyph("module.unknown"));
    }
}
