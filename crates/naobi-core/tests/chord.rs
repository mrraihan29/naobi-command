//! KeyChord text: what `Display` prints, `parse` reads back.
use naobi_core::chord::KeyChord;

// @R-008
#[test]
fn r008_chord_display_round_trips_through_parse() {
    for text in [
        "Alt+Space",
        "Ctrl+Space",
        "Ctrl+Alt+Left",
        "Ctrl+Alt+Shift+Win+F12",
        "Ctrl+Shift+A",
        "Alt+7",
        "Esc",
    ] {
        let chord = KeyChord::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(chord.to_string(), text);
        assert_eq!(KeyChord::parse(&chord.to_string()).ok(), Some(chord));
    }
    // Modifiers always print in one order, whatever order they were typed in.
    let chord = KeyChord::parse("alt+ctrl+left").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(chord.to_string(), "Ctrl+Alt+Left");
}
