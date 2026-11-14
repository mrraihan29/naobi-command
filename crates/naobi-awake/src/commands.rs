//! The Awake commands as Launcher items: a list of the usual ones, and the typed form
//! (`awake 45m`) as one more item. Running one hands the command to the app, which owns the state.
use crate::grammar::{parse, Cmd};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::Arc;

const MINUTE_MS: i64 = 60_000;

pub struct AwakeCommands {
    run: Arc<dyn Fn(Cmd) + Send + Sync>,
}

impl AwakeCommands {
    pub fn new(run: Arc<dyn Fn(Cmd) + Send + Sync>) -> Self {
        Self { run }
    }
}

fn id_of(c: Cmd) -> String {
    match c {
        Cmd::On => "awake.on".into(),
        Cmd::Off => "awake.off".into(),
        Cmd::For(ms) => format!("awake.for.{}", ms / MINUTE_MS),
        Cmd::Display(on) => format!("awake.display.{}", if on { "on" } else { "off" }),
    }
}

fn cmd_of(id: &str) -> Option<Cmd> {
    let rest = id.strip_prefix("awake.")?;
    Some(match rest {
        "on" => Cmd::On,
        "off" => Cmd::Off,
        "display.on" => Cmd::Display(true),
        "display.off" => Cmd::Display(false),
        minutes => Cmd::For(minutes.strip_prefix("for.")?.parse::<i64>().ok()? * MINUTE_MS),
    })
}

fn duration(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} minutes"),
        (1, 0) => "1 hour".into(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn title(c: Cmd) -> String {
    match c {
        Cmd::On => "Keep Awake Until Turned Off".into(),
        Cmd::Off => "Turn Awake Off".into(),
        Cmd::For(ms) => format!("Keep Awake for {}", duration(ms / MINUTE_MS)),
        Cmd::Display(true) => "Awake: Keep the Display On".into(),
        Cmd::Display(false) => "Awake: Let the Display Sleep".into(),
    }
}

fn item(c: Cmd) -> Item {
    Item {
        id: ItemId(id_of(c)),
        title: title(c),
        // The words that make the same command, for whoever types them.
        subtitle: match c {
            Cmd::On => "awake on".into(),
            Cmd::Off => "awake off".into(),
            Cmd::For(ms) => format!("awake {}m", ms / MINUTE_MS),
            Cmd::Display(on) => format!("awake display {}", if on { "on" } else { "off" }),
        },
        keywords: vec!["awake".into(), "caffeine".into(), "sleep".into()],
        alias: None,
        kind: ItemKind::Command,
        primary: ActionId("run".into()),
        secondary: None,
    }
}

impl CommandSource for AwakeCommands {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        // The list: 30 minutes, 1 hour, 2 hours, until turned off, off, and the display choice.
        let mut items: Vec<Item> = [
            Cmd::For(30 * MINUTE_MS),
            Cmd::For(60 * MINUTE_MS),
            Cmd::For(120 * MINUTE_MS),
            Cmd::On,
            Cmd::Off,
            Cmd::Display(true),
            Cmd::Display(false),
        ]
        .into_iter()
        .map(item)
        .collect();
        // A typed custom duration ("awake 45m") is one more item, put first so it is what Enter runs.
        if let Some(c) = parse(&q.text) {
            if !items.iter().any(|i| i.id.0 == id_of(c)) {
                items.insert(0, item(c));
            }
        }
        items.into()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("run".into()),
            label: "Run".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        match cmd_of(&id.0) {
            Some(c) => {
                (self.run)(c);
                Outcome::Effects(vec![Effect::Hide])
            }
            None => Outcome::Done,
        }
    }
}
