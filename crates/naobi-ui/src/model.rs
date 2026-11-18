//! Pure mapping from `naobi_core::ui::Screen` to the Slint models. No window is needed, so these
//! run in `cargo test -p naobi-ui`.
use crate::generated::{ActionLine, Line};
use naobi_core::tokens::Rgba;
use naobi_core::ui::{ActionPanelView, PreviewView, RowView, Screen};
use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer, SharedString};

/// The Launcher's height at 100% text scale, logical px (750 x 475, DESIGN_SYSTEM.md).
pub const BASE_HEIGHT: f32 = 475.0;
/// The Launcher may take at most this share of the work area height (ui.text_scale rule).
pub const WORK_AREA_SHARE: f32 = 0.9;

/// A result row's height at 100% text scale, logical px (D-042).
pub const ROW_HEIGHT: f32 = 40.0;
/// A section header's height at 100% text scale, logical px (D-042).
pub const SECTION_HEIGHT: f32 = 28.0;

/// The list lines, the line index of the selected row (-1 when there is none), where that row sits
/// and how tall all the lines are, in logical px.
pub struct Lines {
    pub items: Vec<Line>,
    pub selected: i32,
    pub selected_top: f32,
    pub selected_bottom: f32,
    pub height: f32,
}

pub fn color(c: Rgba) -> Color {
    Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

fn swatch(hex: u32) -> Color {
    Color::from_rgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Window height in logical px: the base height scaled by `ui.text_scale`, clamped to 90% of the
/// work area. `work_height` is 0 when the work area is unknown, which means no clamp.
pub fn frame_height(text_scale: f32, work_height: f32) -> f32 {
    let base = BASE_HEIGHT * text_scale;
    if work_height > 0.0 {
        base.min(WORK_AREA_SHARE * work_height)
    } else {
        base
    }
}

/// Section headers and rows, flattened in display order. Headers are lines but not rows.
pub fn lines(s: &Screen) -> Lines {
    let mut items = Vec::new();
    let mut selected = -1;
    let mut row = 0;
    let (row_height, section_height) = (ROW_HEIGHT * s.text_scale, SECTION_HEIGHT * s.text_scale);
    let (mut top, mut selected_top, mut selected_bottom) = (0.0, 0.0, 0.0);
    for section in &s.sections {
        if let Some(header) = &section.header {
            items.push(Line {
                is_header: true,
                title: header.as_str().into(),
                top,
                height: section_height,
                ..Default::default()
            });
            top += section_height;
        }
        for r in &section.rows {
            if row == s.selected {
                selected = items.len() as i32;
                selected_top = top;
                selected_bottom = top + row_height;
            }
            let icon = r.icon.as_deref().and_then(icon_image);
            let glyph = r
                .icon_key
                .as_deref()
                .and_then(|k| k.strip_prefix("g:"))
                .map(crate::glyphs::glyph);
            // A run history row is a command line like the typed one, with the history glyph on the right.
            let trailing = (r.icon_key.as_deref() == Some("g:run.history")).then_some(glyph);
            let glyph = trailing.map(|_| crate::glyphs::glyph("run")).or(glyph);
            items.push(Line {
                is_header: false,
                title: r.title.as_str().into(),
                subtitle: r.subtitle.as_str().into(),
                hint: r.hint.as_deref().unwrap_or_default().into(),
                label: accessible_label(r).into(),
                selected: row == s.selected,
                calculator: r.calculator,
                disabled: r.disabled,
                armed: r.armed,
                pinned: r.pinned,
                has_swatch: r.swatch.is_some(),
                swatch: r.swatch.map_or(Color::from_argb_u8(0, 0, 0, 0), swatch),
                has_icon: icon.is_some(),
                icon: icon.unwrap_or_default(),
                has_glyph: glyph.is_some(),
                glyph: glyph.map_or("", |g| g.0).into(),
                glyph_region: glyph.map_or("", |g| g.1).into(),
                glyph_trailing: trailing.flatten().map_or("", |g| g.0).into(),
                row: row as i32,
                top,
                height: row_height,
            });
            top += row_height;
            row += 1;
        }
    }
    Lines {
        items,
        selected,
        selected_top,
        selected_bottom,
        height: top,
    }
}

/// The accessible name of a row: title, subtitle and kind; an empty part is left out, and an
/// armed row adds its confirmation.
pub fn accessible_label(r: &RowView) -> String {
    // Pinned and unavailable are shown by colour or opacity only, so they are named here too.
    let mut parts = vec![r.title.as_str(), r.subtitle.as_str(), r.kind.as_str()];
    if r.pinned {
        parts.push("Pinned");
    }
    if r.disabled {
        parts.push("Unavailable");
    }
    let mut name = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if r.armed {
        name.push_str(", Press Enter again to confirm");
    }
    name
}

/// What the polite live region announces next to a Notice: the result count.
pub fn count_text(s: &Screen) -> String {
    match s
        .sections
        .iter()
        .map(|section| section.rows.len())
        .sum::<usize>()
    {
        0 => "No results".into(),
        1 => "1 result".into(),
        n => format!("{n} results"),
    }
}

pub fn actions(p: &ActionPanelView) -> Vec<ActionLine> {
    p.actions
        .iter()
        .enumerate()
        .map(|(i, a)| ActionLine {
            label: a.label.as_str().into(),
            shortcut: a.shortcut.as_deref().unwrap_or_default().into(),
            reason: a.reason.as_deref().unwrap_or_default().into(),
            selected: i == p.selected,
        })
        .collect()
}

/// (kind, text, picture) for the preview pane. Kind 0 is no preview; the others are listed in
/// `ui/app.slint`. An image whose pixel count does not match its size is shown as an error.
pub fn preview(p: Option<&PreviewView>) -> (i32, SharedString, Image) {
    let blank = || (SharedString::default(), Image::default());
    match p {
        None => (0, blank().0, blank().1),
        Some(PreviewView::Text(t)) => (1, t.as_str().into(), Image::default()),
        Some(PreviewView::Image {
            width,
            height,
            rgba,
        }) => match image(*width, *height, rgba) {
            Some(img) => (2, blank().0, img),
            None => (4, "Cannot read this entry".into(), Image::default()),
        },
        Some(PreviewView::Error(message)) => (4, message.as_str().into(), Image::default()),
        Some(PreviewView::Empty) => (5, blank().0, Image::default()),
    }
}

/// A row's icon. The shell's pictures carry premultiplied alpha.
fn icon_image(icon: &naobi_core::ui::Icon) -> Option<Image> {
    pixels(icon.w, icon.h, &icon.rgba).map(Image::from_rgba8_premultiplied)
}

fn image(width: u32, height: u32, rgba: &[u8]) -> Option<Image> {
    pixels(width, height, rgba).map(Image::from_rgba8)
}

fn pixels(width: u32, height: u32, rgba: &[u8]) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    if rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let (pixels, _) = rgba.as_chunks::<4>();
    for (dst, src) in buffer.make_mut_slice().iter_mut().zip(pixels) {
        *dst = Rgba8Pixel {
            r: src[0],
            g: src[1],
            b: src[2],
            a: src[3],
        };
    }
    Some(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::ui::{ActionView, Section, ViewModel};

    fn row(title: &str) -> RowView {
        RowView {
            kind: "Application".into(),
            title: title.into(),
            subtitle: "App".into(),
            hint: None,
            calculator: false,
            disabled: false,
            armed: false,
            pinned: false,
            swatch: None,
            icon_key: None,
            icon: None,
        }
    }

    fn screen(sections: Vec<Section>, selected: usize) -> Screen {
        let mut s = Screen::from(ViewModel::default());
        s.sections = sections;
        s.selected = selected;
        s
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn frame_height_scales_then_clamps_to_ninety_percent_of_work_area() {
        assert!(close(frame_height(1.0, 0.0), 475.0)); // unknown work area: no clamp
        assert!(close(frame_height(1.25, 0.0), 593.75));
        assert!(close(frame_height(1.25, 1000.0), 593.75)); // 900 > 593.75: not clamped
        assert!(close(frame_height(1.25, 600.0), 540.0)); // 0.9 x 600 < 593.75: clamped
        assert!(close(frame_height(1.0, 400.0), 360.0));
        assert!(close(frame_height(1.0, 527.8), 475.0)); // 0.9 x 527.8 = 475.02: base kept
        assert!(close(frame_height(1.0, 527.0), 474.3)); // 0.9 x 527 = 474.3: clamped
    }

    // @R-007
    #[test]
    fn r007_a_line_carries_its_row_number_and_headers_do_not_count() {
        let sec = |h: Option<&str>, rows: &[&str]| Section {
            header: h.map(str::to_owned),
            rows: rows.iter().map(|t| row(t)).collect(),
        };
        let s = screen(vec![sec(None, &["a", "b"]), sec(Some("Files"), &["c"])], 2);
        let l = lines(&s);
        let rows: Vec<(bool, i32)> = l.items.iter().map(|x| (x.is_header, x.row)).collect();
        // a, b, the "Files" header, c: a click on c reports row 2, the index the engine selects by.
        assert_eq!(rows, [(false, 0), (false, 1), (true, 0), (false, 2)]);
        assert_eq!(l.selected, 3);
    }

    // @D-042
    #[test]
    fn d042_headers_are_28_rows_40_and_both_follow_the_text_scale() {
        let sections = vec![
            Section {
                header: None,
                rows: vec![row("a")],
            },
            Section {
                header: Some("Files".into()),
                rows: vec![row("b")],
            },
        ];
        let mut s = screen(sections, 1);
        let l = lines(&s);
        let boxes: Vec<(f32, f32)> = l.items.iter().map(|x| (x.top, x.height)).collect();
        assert_eq!(boxes, [(0.0, 40.0), (40.0, 28.0), (68.0, 40.0)]);
        assert_eq!(
            (l.selected_top, l.selected_bottom, l.height),
            (68.0, 108.0, 108.0)
        );
        s.text_scale = 1.25;
        let l = lines(&s);
        let boxes: Vec<(f32, f32)> = l.items.iter().map(|x| (x.top, x.height)).collect();
        assert_eq!(boxes, [(0.0, 50.0), (50.0, 35.0), (85.0, 50.0)]);
        assert_eq!(
            (l.selected_top, l.selected_bottom, l.height),
            (85.0, 135.0, 135.0)
        );
    }

    #[test]
    fn color_keeps_each_channel_and_the_alpha_in_order() {
        let c = color(Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        });
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (1, 2, 3, 4));
    }

    #[test]
    fn line_fields_map_one_to_one_from_the_row() {
        let mut r = row("Notepad");
        r.kind = "Application".into();
        r.subtitle = "C:\\Windows\\notepad.exe".into();
        r.hint = Some("Copy".into());
        r.calculator = true;
        r.disabled = true;
        r.armed = true;
        r.pinned = true;
        r.swatch = Some(0x0011_2233);
        let mut plain = row("a.txt");
        plain.swatch = None;
        let out = lines(&screen(
            vec![
                Section {
                    header: Some("Applications".into()),
                    rows: vec![r],
                },
                Section {
                    header: None,
                    rows: vec![plain],
                },
            ],
            0,
        ));
        assert_eq!(out.items.len(), 3);
        let header = &out.items[0];
        assert!(header.is_header);
        assert_eq!(header.title.as_str(), "Applications");
        let item = &out.items[1];
        assert!(!item.is_header && item.selected);
        assert_eq!(item.title.as_str(), "Notepad");
        assert_eq!(item.subtitle.as_str(), "C:\\Windows\\notepad.exe");
        assert_eq!(item.hint.as_str(), "Copy");
        assert_eq!(
            item.label.as_str(),
            "Notepad, C:\\Windows\\notepad.exe, Application, Pinned, Unavailable, Press Enter again to confirm"
        );
        assert!(item.calculator && item.disabled && item.armed && item.pinned);
        assert!(item.has_swatch);
        assert_eq!(
            (item.swatch.red(), item.swatch.green(), item.swatch.blue()),
            (0x11, 0x22, 0x33)
        );
        assert!(!item.has_icon, "no icon loaded: the placeholder shows");
        let last = &out.items[2];
        assert!(!last.selected && !last.has_swatch && !last.pinned);
        assert_eq!(last.hint.as_str(), "");
        assert_eq!(last.swatch.alpha(), 0);
    }

    #[test]
    fn selection_index_counts_lines_including_headers() {
        let out = lines(&screen(
            vec![
                Section {
                    header: Some("Applications".into()),
                    rows: vec![row("Notepad"), row("Calculator")],
                },
                Section {
                    header: Some("Files".into()),
                    rows: vec![row("a.txt")],
                },
            ],
            2,
        ));
        assert_eq!(out.items.len(), 5);
        assert!(out.items[0].is_header && out.items[3].is_header);
        assert_eq!(out.selected, 4);
        assert!(out.items[4].selected && !out.items[2].selected);
        assert_eq!(out.items[4].title.as_str(), "a.txt");
    }

    #[test]
    fn selection_out_of_range_or_no_rows_is_minus_one() {
        assert_eq!(lines(&screen(vec![], 0)).selected, -1);
        let out = lines(&screen(
            vec![Section {
                header: None,
                rows: vec![row("Notepad")],
            }],
            1,
        ));
        assert_eq!(out.selected, -1);
        assert!(!out.items[0].selected);
    }

    #[test]
    fn count_text_names_zero_one_and_many_results() {
        assert_eq!(count_text(&screen(vec![], 0)), "No results");
        let one = vec![Section {
            header: None,
            rows: vec![row("a")],
        }];
        assert_eq!(count_text(&screen(one, 0)), "1 result");
        let three = vec![
            Section {
                header: Some("x".into()),
                rows: vec![row("a"), row("b")],
            },
            Section {
                header: None,
                rows: vec![row("c")],
            },
        ];
        assert_eq!(count_text(&screen(three, 0)), "3 results");
    }

    #[test]
    fn accessible_name_skips_empty_parts_and_adds_the_armed_prompt() {
        assert_eq!(
            accessible_label(&row("Notepad")),
            "Notepad, App, Application"
        );
        let mut no_subtitle = row("Notepad");
        no_subtitle.subtitle = String::new();
        assert_eq!(accessible_label(&no_subtitle), "Notepad, Application");
        let mut no_kind = row("Notepad");
        no_kind.kind = String::new();
        assert_eq!(accessible_label(&no_kind), "Notepad, App");
        let mut pinned = row("Notepad");
        pinned.pinned = true;
        assert_eq!(
            accessible_label(&pinned),
            "Notepad, App, Application, Pinned"
        );
        let mut unavailable = row("Notepad");
        unavailable.disabled = true;
        assert_eq!(
            accessible_label(&unavailable),
            "Notepad, App, Application, Unavailable"
        );
        let mut armed = row("Restart: press Enter again to confirm");
        armed.armed = true;
        assert_eq!(
            accessible_label(&armed),
            "Restart: press Enter again to confirm, App, Application, Press Enter again to confirm"
        );
    }

    #[test]
    fn actions_keep_labels_shortcuts_reasons_and_one_selection() {
        let panel = ActionPanelView {
            filter: String::new(),
            actions: vec![
                ActionView {
                    label: "Open".into(),
                    shortcut: Some("Enter".into()),
                    reason: None,
                },
                ActionView {
                    label: "Uninstall".into(),
                    shortcut: None,
                    reason: Some("Not available".into()),
                },
            ],
            selected: 1,
        };
        let out = actions(&panel);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].label.as_str(), "Open");
        assert_eq!(out[0].shortcut.as_str(), "Enter");
        assert_eq!(out[0].reason.as_str(), "");
        assert!(!out[0].selected);
        assert_eq!(out[1].label.as_str(), "Uninstall");
        assert_eq!(out[1].shortcut.as_str(), "");
        assert_eq!(out[1].reason.as_str(), "Not available");
        assert!(out[1].selected);
    }

    #[test]
    fn preview_kinds_match_the_slint_codes() {
        let text = preview(Some(&PreviewView::Text("hello".into())));
        assert_eq!((text.0, text.1.as_str()), (1, "hello"));
        let error = preview(Some(&PreviewView::Error("Cannot read this entry".into())));
        assert_eq!((error.0, error.1.as_str()), (4, "Cannot read this entry"));
        assert_eq!(preview(Some(&PreviewView::Empty)).0, 5);
        assert_eq!(preview(None).0, 0);
    }

    #[test]
    fn image_preview_keeps_size_and_pixels_and_needs_exact_length() {
        let ok = PreviewView::Image {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        let (kind, _, picture) = preview(Some(&ok));
        assert_eq!(kind, 2);
        assert_eq!((picture.size().width, picture.size().height), (2, 1));
        let pixels = picture.to_rgba8().expect("image has pixels");
        assert_eq!(
            pixels.as_slice(),
            &[
                Rgba8Pixel {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4
                },
                Rgba8Pixel {
                    r: 5,
                    g: 6,
                    b: 7,
                    a: 8
                },
            ]
        );
        let short = PreviewView::Image {
            width: 2,
            height: 1,
            rgba: vec![0; 7],
        };
        let (kind, text, _) = preview(Some(&short));
        assert_eq!((kind, text.as_str()), (4, "Cannot read this entry"));
        // Non-square: width x height x 4 = 24 bytes; width / height x 4 would be 4.
        let wide = PreviewView::Image {
            width: 3,
            height: 2,
            rgba: vec![0; 24],
        };
        assert_eq!(preview(Some(&wide)).0, 2);
        let wide_short = PreviewView::Image {
            width: 3,
            height: 2,
            rgba: vec![0; 4],
        };
        assert_eq!(preview(Some(&wide_short)).0, 4);
    }

    // @R-012
    #[test]
    fn r012_a_loaded_icon_shows_and_a_malformed_one_leaves_the_placeholder() {
        let icon = |w: u32, h: u32, len: usize| {
            Some(std::sync::Arc::new(naobi_core::ui::Icon {
                w,
                h,
                rgba: vec![255; len],
            }))
        };
        let mut good = row("Notepad");
        good.icon = icon(2, 2, 16);
        let mut short = row("Broken");
        short.icon = icon(2, 2, 15);
        let out = lines(&screen(
            vec![Section {
                header: None,
                rows: vec![good, short, row("None")],
            }],
            0,
        ));
        let has: Vec<bool> = out.items.iter().map(|l| l.has_icon).collect();
        assert_eq!(has, [true, false, false]);
    }
}
