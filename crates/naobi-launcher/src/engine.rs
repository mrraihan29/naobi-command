//! The launcher engine: root search, the view stack, the action panel, armed commands and the Esc
//! ladder. `handle` returns the Effects for one Owner event; `screen` returns what the window draws.
//! No Win32 and no Slint here: the controller executes the effects and posts the screen.
use crate::armed::Armed;
use crate::esc::{esc_step, EscStep, Level};
use crate::frecency::Frecency;
use crate::ports::Clock;
use crate::rank::{Hit, Ranker, Source, EMPTY_QUERY_ROWS, MAX_FILE_ROWS, MAX_ROWS};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::ui::{
    ActionPanelView, ActionView, EmptyView, FooterView, Key, KeyChord, NoticeView, PreviewView,
    RowView, Screen, Section, UiEvent, ViewBar, DEFAULT_ROW_HEIGHT,
};
use naobi_core::view_source::{Fact, Preview, ViewSource};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Root,
    /// Pushed from the root by an Enter that returned `Effect::PushView`.
    Pushed(usize),
    /// Opened by its own hotkey; Esc hides it.
    Hotkey(usize),
}

struct Panel {
    target: Hit,
    /// Every action, in display order.
    all: Vec<Action>,
    /// The actions the filter leaves.
    actions: Vec<Action>,
    filter: String,
    selected: usize,
}

pub struct Launcher {
    sources: Vec<Box<dyn CommandSource>>,
    views: Vec<Box<dyn ViewSource>>,
    armed_ids: HashSet<ItemId>,
    frecency: Frecency,
    clock: Box<dyn Clock>,
    ranker: Ranker,
    armed: Armed,
    mode: Mode,
    query: String,
    /// Bumped on every query change and every show; file results carry the generation they answer.
    generation: u64,
    rows: Vec<Hit>,
    files: Vec<Hit>,
    /// The root source that owns the current query (a `>` command line): its rows only, unranked.
    claimed: Option<usize>,
    selected: usize,
    panel: Option<Panel>,
    notice: Option<Notice>,
}

impl Launcher {
    pub fn new(
        sources: Vec<Box<dyn CommandSource>>,
        views: Vec<Box<dyn ViewSource>>,
        frecency: Frecency,
        clock: Box<dyn Clock>,
    ) -> Self {
        // Each source says which of its items need a second Enter (R-020).
        let armed_ids = sources
            .iter()
            .flat_map(|s| s.armed())
            .chain(views.iter().flat_map(|v| v.armed()))
            .collect();
        let mut l = Self {
            sources,
            views,
            armed_ids,
            frecency,
            clock,
            ranker: Ranker::default(),
            armed: Armed::default(),
            mode: Mode::Root,
            query: String::new(),
            generation: 0,
            rows: Vec::new(),
            files: Vec::new(),
            claimed: None,
            selected: 0,
            panel: None,
            notice: None,
        };
        l.refresh();
        l
    }

    /// Marks items whose primary action needs a second Enter (R-020).
    pub fn set_armed(&mut self, ids: impl IntoIterator<Item = ItemId>) {
        self.armed_ids.extend(ids);
    }

    /// Drops the engine's own Notice. The controller shows every Notice (balloon, tray and one
    /// banner), so the engine keeps none once its effects have been handed over.
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Every show starts at the root with an empty query (D-021).
    pub fn show_root(&mut self) {
        self.mode = Mode::Root;
        self.reset();
    }

    /// Shows a view that has its own hotkey. False when no such view is hosted.
    pub fn show_view(&mut self, id: &ViewId) -> bool {
        match self.view_index(id) {
            Some(i) if self.views[i].opened_by_hotkey() => {
                self.mode = Mode::Hotkey(i);
                self.reset();
                true
            }
            _ => false,
        }
    }

    pub fn handle(&mut self, ev: UiEvent) -> Vec<Effect> {
        match ev {
            UiEvent::TextChanged(text) => {
                self.set_query(text);
                vec![]
            }
            UiEvent::Key(chord) => self.key(chord),
            UiEvent::RowActivated(i) => {
                // A click through the open action panel is not for the row under it, and the second
                // click of a double click must not confirm what the first one just armed (R-020).
                if i >= self.row_count()
                    || self.panel.is_some()
                    || self.armed.just_armed(self.clock.now_ms())
                {
                    return vec![];
                }
                if self.selected != i {
                    self.armed.disarm();
                }
                self.selected = i;
                self.run_selected(false)
            }
            UiEvent::RowHovered(i) => {
                if i < self.row_count() && self.panel.is_none() && self.selected != i {
                    self.armed.disarm();
                    self.selected = i;
                }
                vec![]
            }
            UiEvent::Back => {
                if matches!(self.mode, Mode::Pushed(_)) && self.panel.is_none() {
                    self.mode = Mode::Root;
                    self.reset();
                }
                vec![]
            }
            UiEvent::PanelFilter(text) => {
                self.armed.disarm();
                if let Some(p) = &mut self.panel {
                    let wanted = text.to_lowercase();
                    p.actions = p
                        .all
                        .iter()
                        .filter(|a| a.label.to_lowercase().contains(&wanted))
                        .cloned()
                        .collect();
                    p.filter = text;
                    p.selected = 0;
                }
                vec![]
            }
            UiEvent::Dismissed => {
                self.armed.disarm();
                vec![Effect::Hide]
            }
        }
    }

    /// A source's index changed (the app catalog finished): ranks the current query again. The file
    /// rows stay until their new answer, and the selection stays on its item while that is listed.
    pub fn refresh_sources(&mut self) {
        let at = self.selected_hit().map(|h| h.id().clone());
        let files = std::mem::take(&mut self.files);
        self.refresh();
        self.files = files;
        let found = at.and_then(|id| {
            self.rows
                .iter()
                .chain(&self.files)
                .position(|h| *h.id() == id)
        });
        self.selected = found.unwrap_or(0);
    }

    /// The selected row's position.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The query a file search should answer now: its generation and text, or None when the root
    /// is not showing or the query is empty.
    pub fn file_query(&self) -> Option<(u64, String)> {
        (self.mode == Mode::Root && self.claimed.is_none() && !self.query.trim().is_empty())
            .then(|| (self.generation, self.query.clone()))
    }

    /// The root source that takes file rows, if one is registered.
    pub fn file_source(&self) -> Option<usize> {
        self.sources.iter().position(|s| s.takes_files())
    }

    /// Hands in a file search answer. `source` is the root source index that owns these rows, so
    /// their actions run there. Dropped unless `generation` is current, the query is not empty and the
    /// root is showing. The rows trail the sync rows and never reorder them.
    pub fn file_results(&mut self, generation: u64, source: usize, items: Vec<Item>) -> bool {
        if generation != self.generation
            || self.query.is_empty()
            || self.mode != Mode::Root
            || self.claimed.is_some()
        {
            return false;
        }
        self.files = items
            .into_iter()
            .take(MAX_FILE_ROWS)
            .map(|item| Hit {
                src: Source::Root(source),
                item,
                exact: false,
                total: 0.0,
                opens: 0.0,
                matched: Vec::new(),
            })
            .collect();
        self.ranker
            .mark_title_letters(self.query.trim(), &mut self.files);
        true
    }

    pub fn screen(&self) -> Screen {
        let now = self.clock.now_ms();
        let armed = self.armed.armed_item(now);
        let to_row = |h: &Hit| RowView {
            kind: kind_name(h.item.kind).to_owned(),
            title: h.item.title.clone(),
            subtitle: shown_subtitle(&h.item),
            hint: None,
            calculator: h.item.kind == ItemKind::Calculator,
            disabled: false,
            armed: armed == Some(h.id()),
            pinned: false,
            swatch: swatch_of(&h.item),
            icon_key: icon_key_of(&h.item),
            icon: None,
            matched: h.matched.clone(),
            mono_subtitle: matches!(
                h.item.kind,
                ItemKind::File
                    | ItemKind::Calculator
                    | ItemKind::Quicklink
                    | ItemKind::CommandLine
                    | ItemKind::Color
            ),
            right: matches!(h.item.kind, ItemKind::ClipboardEntry | ItemKind::Theme)
                .then(|| h.item.subtitle.clone()),
        };
        let mut sections = Vec::new();
        if !self.rows.is_empty() {
            sections.push(Section {
                header: None,
                rows: self.rows.iter().map(&to_row).collect(),
            });
        }
        if !self.files.is_empty() {
            sections.push(Section {
                header: Some("Files".to_owned()),
                rows: self.files.iter().map(&to_row).collect(),
            });
        }
        // A root source that is still loading shows one status row at the end. It is not a hit, so
        // the selection never reaches it.
        let pending: Vec<&str> = match self.mode {
            Mode::Root if self.claimed.is_none() => {
                self.sources.iter().filter_map(|s| s.pending()).collect()
            }
            _ => Vec::new(),
        };
        let status: Vec<&str> = match self.mode {
            Mode::Root if self.claimed.is_none() && !self.query.trim().is_empty() => {
                self.sources.iter().filter_map(|s| s.status()).collect()
            }
            _ => Vec::new(),
        };
        let rows_of: Vec<&str> = pending.iter().chain(status.iter()).copied().collect();
        if !rows_of.is_empty() {
            sections.push(Section {
                header: None,
                rows: rows_of
                    .iter()
                    .map(|what| RowView {
                        kind: "Status".to_owned(),
                        title: (*what).to_owned(),
                        subtitle: String::new(),
                        hint: None,
                        calculator: false,
                        disabled: true,
                        armed: false,
                        pinned: false,
                        swatch: None,
                        icon_key: None,
                        icon: None,
                        matched: Vec::new(),
                        mono_subtitle: false,
                        right: Some(String::new()),
                    })
                    .collect(),
            });
        }
        let target = self.selected_hit();
        let (preview, facts) = self.preview();
        let footer = FooterView {
            primary: target.and_then(|h| self.primary_label(h, armed == Some(h.id()))),
            secondary: target.and_then(|h| self.secondary_label(h)),
            actions: target.is_some(),
            confirmation: None,
        };
        Screen {
            query: self.query.clone(),
            sections,
            selected: self.selected,
            notice: self.notice.as_ref().map(|n| NoticeView {
                message: format!("{}: {}", n.subject, n.message),
            }),
            action_panel: self.panel.as_ref().map(panel_view),
            footer,
            view: self.view_bar(),
            placeholder: match self.mode {
                Mode::Root => "Search apps and commands, or type > to run one...",
                Mode::Pushed(i) | Mode::Hotkey(i) => self.views[i].placeholder(),
            }
            .to_owned(),
            preview,
            facts,
            theme_preview: self.theme_preview(),
            empty: self.empty_view(),
            loading: !pending.is_empty(),
            reduced_motion: false,
            text_scale: 1.0,
            row_height: DEFAULT_ROW_HEIGHT,
        }
    }

    fn theme_preview(&self) -> Option<String> {
        let (Mode::Pushed(i) | Mode::Hotkey(i)) = self.mode else {
            return None;
        };
        self.views[i].look(self.selected_hit()?.id())
    }

    fn view_bar(&self) -> ViewBar {
        match self.mode {
            Mode::Root => ViewBar {
                name: "Launcher".to_owned(),
                marked: false,
                back: false,
            },
            Mode::Pushed(i) | Mode::Hotkey(i) => ViewBar {
                name: self.views[i].title().to_owned(),
                marked: true,
                back: matches!(self.mode, Mode::Pushed(_)),
            },
        }
    }

    /// A view with nothing to list says so; the root has the "Indexing" status row or nothing.
    fn empty_view(&self) -> Option<EmptyView> {
        let (Mode::Pushed(i) | Mode::Hotkey(i)) = self.mode else {
            // A `>` query with no run history yet (R-024).
            return (self.claimed.is_some() && self.rows.is_empty()).then(|| EmptyView {
                title: "No command lines run yet".to_owned(),
                helper: "Type a command line after > and press Enter".to_owned(),
            });
        };
        if !self.rows.is_empty() {
            return None;
        }
        let title = if self.query.is_empty() {
            self.views[i].empty_title()
        } else {
            "No results"
        };
        Some(EmptyView {
            title: title.to_owned(),
            helper: String::new(),
        })
    }

    fn reset(&mut self) {
        self.query.clear();
        self.panel = None;
        self.armed.disarm();
        self.generation += 1;
        self.refresh();
    }

    fn set_query(&mut self, text: String) {
        self.query = text;
        self.generation += 1;
        self.armed.disarm();
        self.refresh();
    }

    /// Recomputes the sync rows and the letters the query matches in their titles.
    fn refresh(&mut self) {
        self.load_rows();
        let text = if self.claimed.is_some() {
            ""
        } else {
            self.query.trim()
        };
        self.ranker.mark_title_letters(text, &mut self.rows);
    }

    /// Loads the sync rows for the current mode and query. Empty text lists the most frecent.
    fn load_rows(&mut self) {
        let text = self.query.trim().to_owned();
        let q = Query { text: text.clone() };
        let was_claimed = self.claimed.take();
        if self.mode == Mode::Root {
            self.claimed = self.sources.iter().position(|s| s.claims(&q));
        } else if let Some(i) = was_claimed {
            // Leaving a claimed query for a view: the claiming source drops what it holds.
            let _ = self.sources[i].items(&Query::default());
        }
        if let Some(i) = self.claimed {
            self.rows = self.sources[i]
                .items(&q)
                .iter()
                .take(MAX_ROWS)
                .map(|item| Hit {
                    src: Source::Root(i),
                    item: item.clone(),
                    exact: false,
                    total: 0.0,
                    opens: 0.0,
                    matched: Vec::new(),
                })
                .collect();
            self.files.clear();
            self.selected = 0;
            return;
        }
        // Each source hands over a shared snapshot or a fresh list; ranking borrows from these and clones the winners.
        let lists: Vec<(Source, Arc<[Item]>)> = match self.mode {
            Mode::Root => self
                .sources
                .iter()
                .enumerate()
                .map(|(i, s)| (Source::Root(i), s.items(&q)))
                .collect(),
            Mode::Pushed(i) | Mode::Hotkey(i) => {
                vec![(Source::View(i), self.views[i].items(&q))]
            }
        };
        let pool = lists
            .iter()
            .flat_map(|(src, items)| items.iter().map(move |it| (*src, it)));
        let limit = if text.is_empty() {
            EMPTY_QUERY_ROWS
        } else {
            MAX_ROWS
        };
        if let Mode::Pushed(i) | Mode::Hotkey(i) = self.mode {
            if !self.views[i].ranked() {
                self.rows = pool
                    .map(|(src, item)| Hit {
                        src,
                        item: item.clone(),
                        exact: false,
                        total: 0.0,
                        opens: 0.0,
                        matched: Vec::new(),
                    })
                    .take(limit)
                    .collect();
                self.files.clear();
                self.selected = 0;
                return;
            }
        }
        let now = self.clock.now_ms() / 1000;
        let frecency = &self.frecency;
        self.rows = self
            .ranker
            .rank(&text, pool, |id| frecency.decayed_opens(id, now), limit);
        self.files.clear();
        self.selected = 0;
    }

    fn key(&mut self, c: KeyChord) -> Vec<Effect> {
        // Any key but Enter (with or without Ctrl) disarms (R-020). Enter keeps the arming, so
        // the second press can confirm it.
        if c.key != Key::Enter {
            self.armed.disarm();
        }
        match c.key {
            Key::Up => {
                self.move_by(-1);
                vec![]
            }
            Key::Down => {
                self.move_by(1);
                vec![]
            }
            Key::Enter if self.panel.is_some() => self.run_panel(),
            Key::Enter => self.run_selected(c.ctrl),
            Key::Escape => self.escape(),
            Key::K if c.ctrl => {
                self.toggle_panel();
                vec![]
            }
            Key::PageUp => {
                self.move_by(-PAGE_ROWS);
                vec![]
            }
            Key::PageDown => {
                self.move_by(PAGE_ROWS);
                vec![]
            }
            // The field passes Backspace on only when it is empty: that pops a pushed view, like Esc
            // does once the query is clear.
            Key::Backspace if self.query.is_empty() && self.panel.is_none() => {
                if matches!(self.mode, Mode::Pushed(_)) {
                    self.mode = Mode::Root;
                    self.reset();
                }
                vec![]
            }
            // Ctrl+. and Ctrl+X run the selected item's "pin" and "delete" actions, when it has them.
            Key::Period if c.ctrl => self.run_named("pin"),
            Key::X if c.ctrl => self.run_named("delete"),
            _ => vec![],
        }
    }

    fn move_by(&mut self, delta: isize) {
        if let Some(p) = &mut self.panel {
            p.selected = step(p.selected, p.actions.len(), delta);
            return;
        }
        self.armed.disarm();
        self.selected = step(self.selected, self.row_count(), delta);
    }

    /// Enter (primary) or Ctrl+Enter (secondary) on the selected row. An armed command needs a
    /// second Enter inside 5 s; the first one only arms it.
    fn run_selected(&mut self, secondary: bool) -> Vec<Effect> {
        let Some(hit) = self.selected_hit().cloned() else {
            return vec![];
        };
        let action = if secondary {
            match hit.item.secondary.clone() {
                Some(a) => a,
                None => return vec![],
            }
        } else if let Some(armed) = self.armed.armed_action(hit.id(), self.clock.now_ms()) {
            armed.clone()
        } else {
            hit.item.primary.clone()
        };
        self.run(&hit, action)
    }

    /// Runs the selected item's action `name` if it lists one that is enabled.
    fn run_named(&mut self, name: &str) -> Vec<Effect> {
        if self.panel.is_some() {
            return vec![];
        }
        let Some(hit) = self.selected_hit().cloned() else {
            return vec![];
        };
        let actions = self.source(hit.src).actions(hit.id());
        match actions.iter().find(|a| a.id.0 == name && a.enabled.is_ok()) {
            Some(a) => self.run(&hit, a.id.clone()),
            None => vec![],
        }
    }

    fn run_panel(&mut self) -> Vec<Effect> {
        let Some(panel) = self.panel.take() else {
            return vec![];
        };
        let Some(action) = panel.actions.get(panel.selected) else {
            return vec![];
        };
        if action.enabled.is_err() {
            // A disabled action is listed with its reason and cannot run; the panel stays open.
            self.panel = Some(panel);
            return vec![];
        }
        let id = action.id.clone();
        self.run(&panel.target, id)
    }

    /// Runs `action` on `hit` and turns the outcome into effects. A PushView opens the view; a
    /// failed run keeps the Launcher open with its Notice; anything else ends with Hide.
    fn run(&mut self, hit: &Hit, action: ActionId) -> Vec<Effect> {
        // R-020: this is the one gate. Enter, Ctrl+Enter and the action panel all end here, so an
        // armed command runs only on the second press of the same action within 5 s.
        let now = self.clock.now_ms();
        let needs_arm = self.armed_ids.contains(hit.id())
            || self.source(hit.src).needs_confirm(hit.id(), &action);
        if needs_arm && !self.armed.confirm(hit.id(), &action, now) {
            self.armed.arm(hit.id().clone(), action, now);
            return vec![];
        }
        let mut cx = Cx::default();
        let outcome = self.source(hit.src).run(hit.id(), action, &mut cx);
        let failed = matches!(outcome, Outcome::Failed(_));
        let stay = matches!(outcome, Outcome::Refresh | Outcome::Stay { .. });
        let mut effects = cx.take();
        let mut select = None;
        match outcome {
            Outcome::Done | Outcome::Refresh => {}
            Outcome::Stay {
                effects: more,
                select: row,
            } => {
                effects.extend(more);
                select = row;
            }
            Outcome::Effects(more) => effects.extend(more),
            Outcome::Failed(n) => effects.push(Effect::Notice(n)),
        }
        // A calculator, inline color, or quicklink row exists only for the text just typed, and a
        // file row is never ranked (it trails the list): nothing to rank by later, so it must not
        // fill the table.
        let ephemeral = matches!(
            hit.item.kind,
            ItemKind::Calculator
                | ItemKind::Color
                | ItemKind::Quicklink
                | ItemKind::File
                | ItemKind::CommandLine
                | ItemKind::Window
        );
        if matches!(hit.src, Source::Root(_)) && !failed && !ephemeral {
            let now = self.clock.now_ms() / 1000;
            if self.frecency.record(hit.id(), now).is_err() {
                effects.push(Effect::Notice(Notice {
                    subject: "Usage".to_owned(),
                    message: "This open was not counted.".to_owned(),
                    kind: NoticeKind::Warning,
                }));
            }
        }
        let pushed = effects.iter().find_map(|e| match e {
            Effect::PushView(v) => Some(v.clone()),
            _ => None,
        });
        match pushed {
            Some(v) => match self.view_index(&v) {
                Some(i) => {
                    self.mode = Mode::Pushed(i);
                    self.reset();
                }
                None => effects.push(Effect::Notice(Notice {
                    subject: "View".to_owned(),
                    message: "That view is not available.".to_owned(),
                    kind: NoticeKind::Error,
                })),
            },
            None if stay => {
                // The list changed under the selection: rank again and keep the place.
                let at = self.selected;
                self.refresh();
                let wanted = select.and_then(|id| self.rows.iter().position(|h| *h.id() == id));
                self.selected = wanted.unwrap_or(at).min(self.row_count().saturating_sub(1));
            }
            None if !failed && !effects.contains(&Effect::Hide) => effects.push(Effect::Hide),
            None => {}
        }
        for e in &effects {
            if let Effect::Notice(n) = e {
                self.notice = Some(n.clone());
            }
        }
        effects
    }

    /// The Esc ladder (R-012): panel, then Notice, then query, then pop the view, then hide.
    fn escape(&mut self) -> Vec<Effect> {
        let level = match self.mode {
            Mode::Root => Level::Root,
            Mode::Pushed(_) => Level::Pushed,
            Mode::Hotkey(_) => Level::HotkeyOpened,
        };
        match esc_step(
            self.panel.is_some(),
            self.notice.is_some(),
            !self.query.is_empty(),
            level,
        ) {
            EscStep::ClosePanel => {
                self.panel = None;
                vec![]
            }
            EscStep::DismissNotice => {
                self.notice = None;
                vec![]
            }
            EscStep::ClearQuery => {
                self.set_query(String::new());
                vec![]
            }
            EscStep::PopView => {
                self.mode = Mode::Root;
                self.reset();
                vec![]
            }
            EscStep::Hide => vec![Effect::Hide],
        }
    }

    fn toggle_panel(&mut self) {
        if self.panel.is_some() {
            self.panel = None;
            return;
        }
        let Some(target) = self.selected_hit().cloned() else {
            return;
        };
        let mut actions = self.source(target.src).actions(target.id());
        actions.sort_by_key(|a| action_style(&a.id.0).group);
        self.panel = Some(Panel {
            target,
            all: actions.clone(),
            actions,
            filter: String::new(),
            selected: 0,
        });
    }

    fn secondary_label(&self, h: &Hit) -> Option<String> {
        let id = h.item.secondary.as_ref()?;
        self.source(h.src)
            .actions(h.id())
            .into_iter()
            .find(|a| &a.id == id)
            .map(|a| a.label)
    }

    fn primary_label(&self, h: &Hit, armed: bool) -> Option<String> {
        if armed {
            return Some("Confirm".to_owned());
        }
        self.source(h.src)
            .actions(h.id())
            .into_iter()
            .find(|a| a.id == h.item.primary)
            .map(|a| a.label)
    }

    fn preview(&self) -> (Option<PreviewView>, Vec<Fact>) {
        let (Mode::Pushed(i) | Mode::Hotkey(i)) = self.mode else {
            return (None, Vec::new());
        };
        let Some((preview, facts)) = self.selected_hit().and_then(|h| self.views[i].pane(h.id()))
        else {
            return (None, Vec::new());
        };
        let view = match preview {
            Preview::Text(t) => Some(PreviewView::Text(t)),
            Preview::Files(f) => Some(PreviewView::Text(f.join("\n"))),
            Preview::Swatches(s) => Some(PreviewView::Swatches(s)),
            // ponytail: image previews need a PNG decoder and RGBA conversion, not in this engine yet.
            Preview::Image { .. } => None,
            Preview::Rgba { w, h, rgba } => Some(PreviewView::Image {
                width: w,
                height: h,
                rgba,
            }),
        };
        (view, facts)
    }

    fn selected_hit(&self) -> Option<&Hit> {
        self.rows.iter().chain(self.files.iter()).nth(self.selected)
    }

    fn row_count(&self) -> usize {
        self.rows.len() + self.files.len()
    }

    fn source(&self, src: Source) -> &dyn CommandSource {
        match src {
            Source::Root(i) => &*self.sources[i],
            Source::View(i) => &*self.views[i],
        }
    }

    fn view_index(&self, id: &ViewId) -> Option<usize> {
        self.views.iter().position(|v| v.view_id() == *id)
    }
}

/// Rows PageUp and PageDown move by (one screenful of the list).
const PAGE_ROWS: isize = 8;

fn step(i: usize, n: usize, delta: isize) -> usize {
    if n == 0 {
        0
    } else {
        i.saturating_add_signed(delta).min(n - 1)
    }
}

struct ActionStyle {
    glyph: &'static str,
    group: u8,
    destructive: bool,
}

/// How an action looks and where it sits in the panel, by its id (R-026). Other ids are the color
/// formats, which copy.
fn action_style(id: &str) -> ActionStyle {
    let (glyph, group, destructive) = match id {
        "apply" => ("action.apply", 0, false),
        "open" | "focus" => ("action.open", 0, false),
        "run" => ("action.run", 0, false),
        "paste" => ("action.paste", 0, false),
        "folder" => ("action.folder", 1, false),
        "duplicate" => ("action.copy", 1, false),
        "edit" => ("action.edit", 1, false),
        "pin" => ("action.pin", 1, false),
        "minimize" => ("action.minimize", 1, false),
        "close" => ("action.close", 1, false),
        "delete" | "force_quit" => ("action.delete", 2, true),
        _ => ("action.copy", 0, false),
    };
    ActionStyle {
        glyph,
        group,
        destructive,
    }
}

fn panel_view(p: &Panel) -> ActionPanelView {
    let item = &p.target.item;
    ActionPanelView {
        title: item.title.clone(),
        filter: p.filter.clone(),
        actions: p
            .actions
            .iter()
            .map(|a| {
                let style = action_style(&a.id.0);
                let key = if a.id == item.primary {
                    Some("Enter".to_owned())
                } else if item.secondary.as_ref() == Some(&a.id) {
                    Some("Ctrl+Enter".to_owned())
                } else {
                    None
                };
                ActionView {
                    label: a.label.clone(),
                    shortcut: a.shortcut.map(|c| c.to_string()).or(key),
                    reason: a.enabled.clone().err(),
                    glyph: style.glyph.to_owned(),
                    group: style.group,
                    destructive: style.destructive,
                }
            })
            .collect(),
        selected: p.selected,
    }
}

/// What a row's icon is looked up by: an app's id, or a file's extension. Never the file's path.
fn icon_key_of(item: &Item) -> Option<String> {
    match item.kind {
        ItemKind::App => item
            .id
            .0
            .strip_prefix(crate::providers::apps::ID_PREFIX)
            .map(|id| format!("a:{id}")),
        ItemKind::File => {
            let path = item.id.0.strip_prefix("file:")?;
            let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
            let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
            Some(format!("e:{}", ext.to_ascii_lowercase()))
        }
        // Drawn by the UI, never asked of the icon child (`askable` refuses it).
        ItemKind::Command => Some(format!("g:{}", item.id.0)),
        ItemKind::Theme => Some("g:themes.open".to_owned()),
        ItemKind::Window => Some("g:win.open".to_owned()),
        ItemKind::Quicklink => Some("g:quicklink".to_owned()),
        ItemKind::Calculator => Some("g:calc".to_owned()),
        // Never the line: the key must not carry command text.
        ItemKind::CommandLine => Some(
            if item.id.0.starts_with(crate::providers::run::HISTORY_PREFIX) {
                "g:run.history"
            } else {
                "g:run"
            }
            .to_owned(),
        ),
        _ => None,
    }
}

/// A file row shows its folder, because the title is already the file name. Other rows show their
/// subtitle as it is.
fn shown_subtitle(item: &Item) -> String {
    match item.kind {
        ItemKind::File => crate::providers::files::parent(&item.subtitle)
            .unwrap_or(&item.subtitle)
            .to_owned(),
        _ => item.subtitle.clone(),
    }
}

/// A Color item's title is a valid color notation; the row draws that color as its swatch.
fn swatch_of(item: &Item) -> Option<u32> {
    if item.kind != ItemKind::Color {
        return None;
    }
    naobi_core::color::parse(&item.title).map(|c| c.rgb.packed())
}

fn kind_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::App => "Application",
        ItemKind::File => "File",
        ItemKind::Command => "Command",
        ItemKind::Quicklink => "Quicklink",
        ItemKind::Calculator => "Calculator",
        ItemKind::ClipboardEntry => "Clipboard entry",
        ItemKind::Color => "Color",
        ItemKind::CommandLine => "Command line",
        ItemKind::Theme => "Theme",
        ItemKind::Window => "Open window",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-026
    #[test]
    fn r026_each_action_id_has_its_glyph_group_and_danger_style() {
        for (id, glyph, group, destructive) in [
            ("apply", "action.apply", 0, false),
            ("open", "action.open", 0, false),
            ("focus", "action.open", 0, false),
            ("run", "action.run", 0, false),
            ("paste", "action.paste", 0, false),
            ("folder", "action.folder", 1, false),
            ("duplicate", "action.copy", 1, false),
            ("edit", "action.edit", 1, false),
            ("pin", "action.pin", 1, false),
            ("minimize", "action.minimize", 1, false),
            ("close", "action.close", 1, false),
            ("delete", "action.delete", 2, true),
            ("force_quit", "action.delete", 2, true),
            ("hex", "action.copy", 0, false),
        ] {
            let s = action_style(id);
            assert_eq!(
                (s.glyph, s.group, s.destructive),
                (glyph, group, destructive),
                "{id}"
            );
        }
    }

    // @R-031
    #[test]
    fn r031_a_theme_row_draws_the_themes_glyph() {
        let item = Item {
            id: ItemId("theme:paper".into()),
            title: "Paper".into(),
            subtitle: String::new(),
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::Theme,
            primary: ActionId("apply".into()),
            secondary: None,
        };
        assert_eq!(icon_key_of(&item).as_deref(), Some("g:themes.open"));
    }
}
