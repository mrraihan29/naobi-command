//! The calculator row (R-014, D-017) and Calculation History view (issue #2): an expression
//! typed in the root search shows its result, Enter copies it and records `(expr, result)` for
//! `ans` chaining and searchable history.
use crate::calc_history::{CalcEntry, CalcHistory};
use naobi_calc::EvalOptions;
use naobi_core::chord::{KeyChord, CTRL};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::DEFAULT_REM_BASE_PX;
use naobi_core::view_source::{Fact, Preview, ViewSource};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use zeroize::Zeroizing;

const ID_PREFIX: &str = "calc:";
pub const VIEW_ID: &str = "calc-history";
pub const HISTORY_CMD_ID: &str = "calc.history";
pub const ROW_PREFIX: &str = "calc.h:";

pub type SharedCalcHistory = Arc<Mutex<CalcHistory>>;

/// Live calculator settings shared between settings hot-reload and `CalcSource`.
#[derive(Clone)]
pub struct CalcConfig(Arc<AtomicU64>);

impl Default for CalcConfig {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(DEFAULT_REM_BASE_PX.to_bits())))
    }
}

impl CalcConfig {
    pub fn set_rem_base_px(&self, px: f64) {
        let valid = if px.is_finite() && px > 0.0 {
            px
        } else {
            DEFAULT_REM_BASE_PX
        };
        self.0.store(valid.to_bits(), Ordering::Relaxed);
    }

    pub fn rem_base_px(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn contains_ans(text: &str) -> bool {
    text.as_bytes()
        .windows(3)
        .any(|w| w.eq_ignore_ascii_case(b"ans"))
}

pub struct CalcSource {
    history: Option<SharedCalcHistory>,
    config: CalcConfig,
    now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    last: Mutex<Option<(Zeroizing<String>, Zeroizing<String>)>>,
}

impl Default for CalcSource {
    fn default() -> Self {
        Self::new(
            None,
            CalcConfig::default(),
            Box::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64)
            }),
        )
    }
}

impl CalcSource {
    pub fn with_now(now_ms: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self::new(None, CalcConfig::default(), Box::new(now_ms))
    }

    pub fn new(
        history: Option<SharedCalcHistory>,
        config: CalcConfig,
        now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            history,
            config,
            now_ms,
            last: Mutex::new(None),
        }
    }
}

impl CommandSource for CalcSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let ans = if contains_ans(&q.text) {
            self.history.as_ref().and_then(|h| lock(h).latest_ans())
        } else {
            None
        };
        let opts = EvalOptions {
            ans,
            rem_base_px: self.config.rem_base_px(),
        };
        let Some(out) = naobi_calc::evaluate_at_with(&q.text, opts, (self.now_ms)()) else {
            *lock(&self.last) = None;
            return Arc::from(Vec::new());
        };
        *lock(&self.last) = Some((
            Zeroizing::new(q.text.clone()),
            Zeroizing::new(out.text.clone()),
        ));
        vec![Item {
            id: ItemId(format!("{ID_PREFIX}{}", out.text)),
            title: out.text,
            subtitle: q.text.clone(),
            keywords: Vec::new(),
            // The query is this row's alias, so ranking always keeps it and puts it first.
            alias: Some(q.text.clone()),
            kind: ItemKind::Calculator,
            primary: ActionId("copy".into()),
            secondary: None,
        }]
        .into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("copy".into()),
            label: "Copy".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, cx: &mut Cx) -> Outcome {
        let Some(result) = id.0.strip_prefix(ID_PREFIX) else {
            return Outcome::Done;
        };
        cx.copy_text(result.to_owned());
        if let Some(h) = &self.history {
            let expr = lock(&self.last)
                .as_ref()
                .filter(|(_, r)| r.as_str() == result)
                .map(|(e, _)| e.to_string())
                .unwrap_or_else(|| result.to_owned());
            let _ = lock(h).record(&expr, result, (self.now_ms)());
        }
        Outcome::Done
    }
}

/// Root command to open Calculation History.
pub struct CalcHistoryCommands;

impl CommandSource for CalcHistoryCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(HISTORY_CMD_ID.into()),
            title: "Calculation History".into(),
            subtitle: String::new(),
            keywords: vec![
                "calculator history".into(),
                "recent calculations".into(),
                "ans".into(),
            ],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("run".into()),
            secondary: None,
        }]
        .into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0 == HISTORY_CMD_ID {
            Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))])
        } else {
            Outcome::Done
        }
    }
}

fn row_id(id: &ItemId) -> Option<i64> {
    id.0.strip_prefix(ROW_PREFIX)?.parse().ok()
}

fn relative(now_s: i64, then_s: i64) -> String {
    let secs = (now_s - then_s).max(0);
    match secs {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// Searchable Calculation History view (newest first).
pub struct CalcHistoryView {
    history: SharedCalcHistory,
    now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl CalcHistoryView {
    pub fn new(history: SharedCalcHistory, now_ms: Box<dyn Fn() -> i64 + Send + Sync>) -> Self {
        Self { history, now_ms }
    }

    fn find(&self, id: &ItemId) -> Option<CalcEntry> {
        let n = row_id(id)?;
        lock(&self.history).get(n)
    }
}

impl CommandSource for CalcHistoryView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let needle = q.text.trim().to_lowercase();
        lock(&self.history)
            .list()
            .into_iter()
            .filter(|e| {
                needle.is_empty()
                    || e.expr.to_lowercase().contains(&needle)
                    || e.result.to_lowercase().contains(&needle)
            })
            .map(|e| Item {
                id: ItemId(format!("{ROW_PREFIX}{}", e.id)),
                title: e.result.to_string(),
                subtitle: e.expr.to_string(),
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::Calculator,
                primary: ActionId("copy".into()),
                secondary: Some(ActionId("copy_expr".into())),
            })
            .collect()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        let act = |id: &str, label: &str, shortcut: Option<KeyChord>| Action {
            id: ActionId(id.into()),
            label: label.into(),
            shortcut,
            enabled: Ok(()),
        };
        vec![
            act("copy", "Copy Result", None),
            act("copy_expr", "Copy Expression", None),
            act(
                "delete",
                "Delete",
                Some(KeyChord {
                    mods: CTRL,
                    vk: 0x58,
                }),
            ),
        ]
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        let Some(n) = row_id(id) else {
            return Outcome::Done;
        };
        match action.0.as_str() {
            "copy" => {
                if let Some(e) = lock(&self.history).get(n) {
                    cx.copy_text(e.result.to_string());
                }
                Outcome::Done
            }
            "copy_expr" => {
                if let Some(e) = lock(&self.history).get(n) {
                    cx.copy_text(e.expr.to_string());
                }
                Outcome::Done
            }
            "delete" => match lock(&self.history).delete(n) {
                Ok(()) => Outcome::Refresh,
                Err(_) => Outcome::Failed(Notice {
                    subject: "Calculation history".into(),
                    message: "This calculation was not deleted.".into(),
                    kind: NoticeKind::Error,
                }),
            },
            _ => Outcome::Done,
        }
    }
}

impl ViewSource for CalcHistoryView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Calculation history"
    }

    fn opened_by_hotkey(&self) -> bool {
        false
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search calculations..."
    }

    fn empty_title(&self) -> &'static str {
        "No calculations yet"
    }

    fn preview(&self, id: &ItemId) -> Option<Preview> {
        self.pane(id).map(|(p, _)| p)
    }

    fn pane(&self, id: &ItemId) -> Option<(Preview, Vec<Fact>)> {
        let e = self.find(id)?;
        let preview = Preview::Text(format!("{} = {}", e.expr.as_str(), e.result.as_str()));
        let facts = vec![Fact {
            label: "Calculated".into(),
            value: relative((self.now_ms)() / 1000, e.calculated_at / 1000),
        }];
        Some((preview, facts))
    }
}
