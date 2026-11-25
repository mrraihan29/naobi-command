//! The calculator row (R-014, D-017) and Calculation History view (issue #2): an expression
//! typed in the root search shows its result, Enter copies it and records `(expr, result)` for
//! `ans` chaining and searchable history.
use crate::calc_history::{CalcEntry, CalcHistory};
use naobi_calc::{evaluate_currency, CurrencyOutcome, EvalOptions, FxRates, NumberFormat};
use naobi_core::chord::{KeyChord, CTRL};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::settings::DEFAULT_REM_BASE_PX;
use naobi_core::view_source::{Fact, Preview, ViewSource};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};
use zeroize::Zeroizing;

const ID_PREFIX: &str = "calc:";
const FX_PREFIX: &str = "calc.fx:";
const FX_MISSING_ID: &str = "calc.fx:missing";
const MAX_FX_BYTES: u64 = 256 * 1024;
pub const FRANKFURTER_URL: &str = "https://api.frankfurter.dev/v1/latest?base=EUR";
pub const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
pub const FX_REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
pub const REFRESH_CMD_ID: &str = "calc.refresh_rates";
pub const VIEW_ID: &str = "calc-history";
pub const HISTORY_CMD_ID: &str = "calc.history";
pub const ROW_PREFIX: &str = "calc.h:";

/// Port for fetching raw FX rate payloads over hardened HTTPS.
pub trait FxFeed: Send + Sync {
    fn fetch(&self, url: &str) -> Result<String, String>;
}

pub type SharedCalcHistory = Arc<Mutex<CalcHistory>>;

struct CalcSettings {
    rem_base_px: f64,
    default_currency: String,
    number_format: NumberFormat,
    fx_rates: Option<Arc<FxRates>>,
    fx_path: Option<PathBuf>,
    fx_mtime: Option<SystemTime>,
}

impl Default for CalcSettings {
    fn default() -> Self {
        Self {
            rem_base_px: DEFAULT_REM_BASE_PX,
            default_currency: "IDR".to_owned(),
            number_format: NumberFormat::Us,
            fx_rates: None,
            fx_path: None,
            fx_mtime: None,
        }
    }
}

/// Live calculator and currency settings shared between settings hot-reload and `CalcSource`.
#[derive(Clone)]
pub struct CalcConfig(Arc<Mutex<CalcSettings>>);

impl Default for CalcConfig {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(CalcSettings::default())))
    }
}

impl CalcConfig {
    pub fn set_rem_base_px(&self, px: f64) {
        let valid = if px.is_finite() && px > 0.0 {
            px
        } else {
            DEFAULT_REM_BASE_PX
        };
        lock(&self.0).rem_base_px = valid;
    }

    pub fn rem_base_px(&self) -> f64 {
        lock(&self.0).rem_base_px
    }

    pub fn set_currency(&self, default_currency: &str, number_format: &str) {
        let mut s = lock(&self.0);
        let code = default_currency.trim().to_ascii_uppercase();
        if code.len() == 3 && code.bytes().all(|b| b.is_ascii_alphabetic()) {
            s.default_currency = code;
        }
        s.number_format = NumberFormat::parse(number_format).unwrap_or(NumberFormat::Us);
    }

    pub fn set_fx_rates(&self, rates: Option<FxRates>) {
        lock(&self.0).fx_rates = rates.map(Arc::new);
    }

    pub fn fx_path(&self) -> Option<PathBuf> {
        lock(&self.0).fx_path.clone()
    }

    pub fn apply_fx_rates(&self, rates: FxRates, mtime: Option<SystemTime>) {
        let mut s = lock(&self.0);
        s.fx_rates = Some(Arc::new(rates));
        s.fx_mtime = mtime;
    }

    pub fn set_fx_path(&self, path: Option<PathBuf>) {
        {
            let mut s = lock(&self.0);
            s.fx_path = path;
            s.fx_mtime = None;
            s.fx_rates = None;
        }
        self.reload_fx_rates();
    }

    /// Loads `fx-rates.json` from `fx_path` cleanly without any network call.
    pub fn reload_fx_rates(&self) {
        let path = lock(&self.0).fx_path.clone();
        let Some(path) = path else {
            return;
        };
        let (rates, mtime) = read_fx_file(&path);
        let mut s = lock(&self.0);
        s.fx_rates = rates.map(Arc::new);
        s.fx_mtime = mtime;
    }

    fn reload_if_changed(&self) {
        let (path, prev_mtime) = {
            let s = lock(&self.0);
            (s.fx_path.clone(), s.fx_mtime)
        };
        let Some(path) = path else {
            return;
        };
        let cur_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if cur_mtime != prev_mtime {
            self.reload_fx_rates();
        }
    }

    fn currency_snapshot(&self) -> (String, NumberFormat, Option<Arc<FxRates>>) {
        let s = lock(&self.0);
        (
            s.default_currency.clone(),
            s.number_format,
            s.fx_rates.clone(),
        )
    }
}

fn read_fx_file(path: &Path) -> (Option<FxRates>, Option<SystemTime>) {
    let Ok(file) = std::fs::File::open(path) else {
        return (None, None);
    };
    let mtime = file.metadata().and_then(|m| m.modified()).ok();
    let mut bytes = Vec::new();
    if file.take(MAX_FX_BYTES + 1).read_to_end(&mut bytes).is_err()
        || bytes.len() as u64 > MAX_FX_BYTES
    {
        return (None, mtime);
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return (None, mtime);
    };
    (FxRates::from_json(text), mtime)
}

fn write_fx_atomic(path: &Path, json: &str) -> std::io::Result<Option<SystemTime>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(std::fs::metadata(path).and_then(|m| m.modified()).ok())
}

/// Fetches latest currency rates from Frankfurter (primary) with ECB XML fallback,
/// atomically updates `fx-rates.json`, and updates `CalcConfig` immediately on success.
pub fn refresh_rates(feed: &dyn FxFeed, config: &CalcConfig) -> Result<FxRates, Notice> {
    let rates = feed
        .fetch(FRANKFURTER_URL)
        .ok()
        .and_then(|body| FxRates::from_json(&body))
        .or_else(|| {
            feed.fetch(ECB_URL)
                .ok()
                .and_then(|body| FxRates::from_ecb_xml(&body))
        })
        .ok_or_else(|| Notice {
            subject: "Currency rates".into(),
            message: "Could not refresh currency rates from Frankfurter or ECB.".into(),
            kind: NoticeKind::Warning,
        })?;

    let mtime = if let Some(path) = config.fx_path() {
        match write_fx_atomic(&path, &rates.to_json()) {
            Ok(m) => m,
            Err(_) => {
                config.apply_fx_rates(rates, None);
                return Err(Notice {
                    subject: "Currency rates".into(),
                    message: "Fetched currency rates, but could not save fx-rates.json.".into(),
                    kind: NoticeKind::Warning,
                });
            }
        }
    } else {
        None
    };
    config.apply_fx_rates(rates.clone(), mtime);
    Ok(rates)
}

/// True when `auto_refresh` is enabled, no automatic refresh attempt occurred in the last 24h,
/// and `fx-rates.json` is missing, invalid, or older than 24 hours.
pub fn should_auto_refresh(
    fx_path: Option<&Path>,
    auto_refresh: bool,
    last_attempt: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    if !auto_refresh {
        return false;
    }
    if last_attempt.is_some_and(|t| {
        now.duration_since(t)
            .is_ok_and(|age| age < FX_REFRESH_INTERVAL)
    }) {
        return false;
    }
    let Some(path) = fx_path else {
        return false;
    };
    match read_fx_file(path) {
        (Some(_), Some(mtime)) => match now.duration_since(mtime) {
            Ok(age) => age >= FX_REFRESH_INTERVAL,
            // Treat future mtimes (clock skew) as fresh to avoid hammering endpoints.
            Err(_) => false,
        },
        _ => true,
    }
}

/// Root command to refresh cached currency exchange rates on demand.
pub struct FxRefreshCommands {
    run: Arc<dyn Fn() -> Outcome + Send + Sync>,
}

impl FxRefreshCommands {
    pub fn new(run: Arc<dyn Fn() -> Outcome + Send + Sync>) -> Self {
        Self { run }
    }
}

impl CommandSource for FxRefreshCommands {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId(REFRESH_CMD_ID.into()),
            title: "Refresh currency rates".into(),
            subtitle: String::new(),
            keywords: vec!["currency".into(), "exchange rates".into(), "rates".into()],
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
            label: "Refresh".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        if id.0 == REFRESH_CMD_ID {
            (self.run)()
        } else {
            Outcome::Done
        }
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

impl CalcSource {
    fn record_history(&self, result: &str) {
        if let Some(h) = &self.history {
            let expr = lock(&self.last)
                .as_ref()
                .filter(|(_, r)| r.as_str() == result)
                .map(|(e, _)| e.to_string())
                .unwrap_or_else(|| result.to_owned());
            let _ = lock(h).record(&expr, result, (self.now_ms)());
        }
    }
}

impl CommandSource for CalcSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        if q.text
            .split_whitespace()
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("mix"))
        {
            return Arc::from(Vec::new());
        }
        let ans = if contains_ans(&q.text) {
            self.history.as_ref().and_then(|h| lock(h).latest_ans())
        } else {
            None
        };
        let opts = EvalOptions {
            ans,
            rem_base_px: self.config.rem_base_px(),
        };
        if let Some(out) = naobi_calc::evaluate_at_with(&q.text, opts, (self.now_ms)()) {
            *lock(&self.last) = Some((
                Zeroizing::new(q.text.clone()),
                Zeroizing::new(out.text.clone()),
            ));
            return vec![Item {
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
            .into();
        }
        if q.text.bytes().any(|b| b.is_ascii_digit()) {
            self.config.reload_if_changed();
            let (default_cur, fmt, rates) = self.config.currency_snapshot();
            match evaluate_currency(&q.text, &default_cur, fmt, rates.as_deref()) {
                Some(CurrencyOutcome::Converted(conv)) => {
                    *lock(&self.last) = Some((
                        Zeroizing::new(q.text.clone()),
                        Zeroizing::new(conv.raw.clone()),
                    ));
                    return vec![Item {
                        id: ItemId(format!("{FX_PREFIX}{}|{}", conv.raw, conv.display)),
                        title: conv.display,
                        subtitle: conv.subtitle,
                        keywords: Vec::new(),
                        alias: Some(q.text.clone()),
                        kind: ItemKind::Calculator,
                        primary: ActionId("copy".into()),
                        secondary: Some(ActionId("copy_with_currency".into())),
                    }]
                    .into();
                }
                Some(CurrencyOutcome::MissingRates { from, to }) => {
                    *lock(&self.last) = None;
                    return vec![Item {
                        id: ItemId(FX_MISSING_ID.into()),
                        title: "No cached currency rates".into(),
                        subtitle: format!(
                            "Refresh currency rates to convert {from} to {to} (no local fx-rates.json)"
                        ),
                        keywords: Vec::new(),
                        alias: Some(q.text.clone()),
                        kind: ItemKind::Calculator,
                        primary: ActionId("notice".into()),
                        secondary: None,
                    }]
                    .into();
                }
                None => {}
            }
        }
        *lock(&self.last) = None;
        Arc::from(Vec::new())
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        let act = |aid: &str, label: &str| Action {
            id: ActionId(aid.into()),
            label: label.into(),
            shortcut: None,
            enabled: Ok(()),
        };
        if id.0 == FX_MISSING_ID {
            vec![act("notice", "How to refresh rates")]
        } else if id.0.starts_with(FX_PREFIX) {
            vec![
                act("copy", "Copy"),
                act("copy_with_currency", "Copy with currency"),
            ]
        } else {
            vec![act("copy", "Copy")]
        }
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        if id.0 == FX_MISSING_ID {
            return Outcome::Failed(Notice {
                subject: "Currency rates".into(),
                message: "No local rate cache (fx-rates.json) found in data folder. Run Refresh currency rates to fetch rates.".into(),
                kind: NoticeKind::Warning,
            });
        }
        if let Some(rest) = id.0.strip_prefix(FX_PREFIX) {
            let Some((raw, display)) = rest.split_once('|') else {
                return Outcome::Done;
            };
            let copied = if action.0 == "copy_with_currency" {
                display
            } else {
                raw
            };
            cx.copy_text(copied.to_owned());
            self.record_history(raw);
            return Outcome::Done;
        }
        let Some(result) = id.0.strip_prefix(ID_PREFIX) else {
            return Outcome::Done;
        };
        cx.copy_text(result.to_owned());
        self.record_history(result);
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
