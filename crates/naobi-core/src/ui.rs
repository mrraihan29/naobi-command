//! Plain data between the core and the UI adapter (ADR-0002). No framework types.

/// The two Naobi themes; the adapter maps each to its design tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Paper,
    Stone,
}

/// A decoded icon: 8-bit RGBA, row by row (`rgba.len() == w * h * 4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// One result row as shown on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub title: String,
    pub subtitle: String,
}

/// An immutable snapshot of everything the Launcher shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewModel {
    pub query: String,
    pub rows: Vec<Row>,
    pub selected: usize,
}

impl ViewModel {
    /// Moves the selection by `delta`, staying inside the rows.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }
}

/// The full Launcher snapshot: everything the views draw (DESIGN_SYSTEM.md). `ViewModel` converts
/// into one, so the list-only producer keeps working.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub query: String,
    /// Sections in display order. A section without a header is a plain list.
    pub sections: Vec<Section>,
    /// Index of the selected row across all sections (headers do not count).
    pub selected: usize,
    pub notice: Option<NoticeView>,
    pub action_panel: Option<ActionPanelView>,
    pub footer: FooterView,
    pub view: ViewBar,
    /// The search field's hint while it is empty.
    pub placeholder: String,
    pub preview: Option<PreviewView>,
    pub empty: Option<EmptyView>,
    pub loading: bool,
    pub reduced_motion: bool,
    /// `ui.text_scale`: 1.0, 1.125 or 1.25.
    pub text_scale: f32,
}

/// Where the Owner is (R-028).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewBar {
    /// The current view's name, shown in the footer.
    pub name: String,
    /// The search header marks the view: true everywhere but the root.
    pub marked: bool,
    /// The marker has a back control: a pushed view, not one opened by its own hotkey.
    pub back: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub header: Option<String>,
    pub rows: Vec<RowView>,
}

/// One row as shown on screen. `kind` names the row for assistive technology ("Application").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    /// Key hint chip on the right, such as "Copy".
    pub hint: Option<String>,
    /// Calculator result: the title is the result value, the subtitle the expression.
    pub calculator: bool,
    pub disabled: bool,
    pub armed: bool,
    pub pinned: bool,
    /// Color history rows: 0xRRGGBB.
    pub swatch: Option<u32>,
    /// What the row's icon comes from: `a:<app id>` for an app, `e:<extension>` for a file. Never a
    /// path, so asking for an icon cannot touch a file or a network share (T-030).
    pub icon_key: Option<String>,
    /// The icon, once it is loaded.
    pub icon: Option<std::sync::Arc<Icon>>,
    /// Title characters the query matched, ascending (R-025).
    pub matched: Vec<u32>,
    /// The subtitle is technical text, such as a folder or an expression (R-025).
    pub mono_subtitle: bool,
    /// Shown at the right edge instead of the kind, such as a copy time. Empty shows nothing.
    pub right: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPanelView {
    pub filter: String,
    pub actions: Vec<ActionView>,
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionView {
    pub label: String,
    pub shortcut: Option<String>,
    /// `Some(reason)` marks the action unavailable; it is listed dimmed with the reason.
    pub reason: Option<String>,
}

/// A Notice banner. The message names its subject; the banner is always announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeView {
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FooterView {
    /// Label of the primary action, shown after the "Enter" chip. `None` shows only the name.
    pub primary: Option<String>,
    /// Label of the secondary action, shown after a "Ctrl Enter" chip.
    pub secondary: Option<String>,
    /// Shows the "Ctrl K Actions" chip.
    pub actions: bool,
    /// A short confirmation that replaces the primary action for a moment, such as "Copied". It is
    /// not a Notice: nothing is raised, nothing sticks.
    pub confirmation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewView {
    Text(String),
    /// RGBA8 pixels, row by row; `rgba.len()` must be `width * height * 4`.
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    /// Message text, for example "Cannot read this entry".
    Error(String),
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyView {
    pub title: String,
    pub helper: String,
}

impl From<Row> for RowView {
    fn from(r: Row) -> Self {
        Self {
            kind: String::new(),
            title: r.title,
            subtitle: r.subtitle,
            hint: None,
            calculator: false,
            disabled: false,
            armed: false,
            pinned: false,
            swatch: None,
            icon_key: None,
            icon: None,
            matched: Vec::new(),
            mono_subtitle: false,
            right: None,
        }
    }
}

impl From<ViewModel> for Screen {
    fn from(vm: ViewModel) -> Self {
        let rows: Vec<RowView> = vm.rows.into_iter().map(RowView::from).collect();
        Self {
            query: vm.query,
            sections: if rows.is_empty() {
                Vec::new()
            } else {
                vec![Section { header: None, rows }]
            },
            selected: vm.selected,
            notice: None,
            action_panel: None,
            footer: FooterView::default(),
            view: ViewBar {
                name: "Launcher".into(),
                ..ViewBar::default()
            },
            placeholder: "Search apps and commands, or type > to run one...".into(),
            preview: None,
            empty: None,
            loading: false,
            reduced_motion: false,
            text_scale: 1.0,
        }
    }
}

/// The keys the Launcher window reports; their meaning is decided outside the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Escape,
    Backspace,
    /// Ctrl+. (pin or unpin the clipboard entry).
    Period,
    /// Ctrl+X (delete the clipboard entry when the field has no selection).
    X,
    /// Ctrl+K (open or close the action panel).
    K,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What the Owner did in the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    TextChanged(String),
    Key(KeyChord),
    RowActivated(usize),
    /// The pointer moved onto a row: it becomes the selected row, like the arrow keys.
    RowHovered(usize),
    /// The view marker's back control: leaves a pushed view.
    Back,
    Dismissed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: usize) -> ViewModel {
        let row = |i| Row {
            title: format!("r{i}"),
            subtitle: String::new(),
        };
        ViewModel {
            rows: (0..n).map(row).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn selection_stays_inside_the_rows() {
        let mut vm = rows(3);
        vm.move_selection(-1);
        assert_eq!(vm.selected, 0);
        vm.move_selection(1);
        vm.move_selection(1);
        vm.move_selection(1);
        assert_eq!(vm.selected, 2);
    }

    #[test]
    fn selection_on_no_rows_is_zero() {
        let mut vm = rows(0);
        vm.move_selection(1);
        assert_eq!(vm.selected, 0);
    }

    #[test]
    fn view_model_becomes_one_plain_section() {
        let mut vm = rows(2);
        vm.selected = 1;
        let screen = Screen::from(vm);
        assert_eq!(screen.sections.len(), 1);
        assert_eq!(screen.sections[0].header, None);
        assert_eq!(screen.sections[0].rows[1].title, "r1");
        assert_eq!(screen.selected, 1);
        assert_eq!(screen.text_scale, 1.0);
        assert!(Screen::from(rows(0)).sections.is_empty());
    }
}
