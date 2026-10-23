//! Plain data between the core and the UI adapter (ADR-0002). No framework types.

/// The two Naobi themes; the adapter maps each to its design tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Paper,
    Stone,
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

/// The keys the Launcher window reports; their meaning is decided outside the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
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
}
