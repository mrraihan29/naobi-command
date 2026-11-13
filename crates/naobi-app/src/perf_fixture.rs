//! Synthetic index for the latency item (feature `perf`): 10,000 deterministic titles that the real
//! launcher engine ranks.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};

const WORDS: [&str; 64] = [
    "visual",
    "studio",
    "code",
    "terminal",
    "profile",
    "chrome",
    "notes",
    "calendar",
    "mail",
    "photos",
    "music",
    "video",
    "maps",
    "weather",
    "calculator",
    "clock",
    "settings",
    "system",
    "network",
    "bluetooth",
    "display",
    "sound",
    "storage",
    "update",
    "security",
    "privacy",
    "backup",
    "recorder",
    "snipping",
    "paint",
    "editor",
    "browser",
    "player",
    "reader",
    "writer",
    "sheet",
    "slides",
    "draw",
    "design",
    "build",
    "deploy",
    "docker",
    "git",
    "cargo",
    "python",
    "node",
    "java",
    "rust",
    "swift",
    "kotlin",
    "monitor",
    "task",
    "manager",
    "explorer",
    "search",
    "launcher",
    "clipboard",
    "history",
    "download",
    "upload",
    "cloud",
    "drive",
    "chat",
    "zoom",
];

/// The synthetic index as a root source: the real engine ranks it, so the latency runs cross the
/// engine's own path (the candidate clone per keystroke included).
pub struct PerfSource {
    items: Vec<Item>,
}

impl PerfSource {
    pub fn new(n: usize) -> Self {
        let items = (0..n)
            .map(|i| {
                let (a, b) = (WORDS[i % 64], WORDS[(i * 7 + 3) % 64]);
                Item {
                    id: ItemId(format!("perf:{i}")),
                    title: format!("{a} {b} {i}"),
                    subtitle: format!("C:\\Program Files\\{a}\\{b}.exe"),
                    keywords: Vec::new(),
                    alias: None,
                    kind: ItemKind::App,
                    primary: ActionId("open".into()),
                    secondary: None,
                }
            })
            .collect();
        PerfSource { items }
    }
}

impl CommandSource for PerfSource {
    fn items(&self, _q: &Query) -> Vec<Item> {
        self.items.clone()
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("open".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    /// Nothing is launched: the harness only types.
    fn run(&self, _id: &ItemId, _action: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Done
    }
}

/// Tier 2 target (`NAOBI_TEST_TARGET` = a script path): one item, "naobitarget", that opens it.
pub struct TestTarget(pub String);

impl CommandSource for TestTarget {
    fn items(&self, _q: &Query) -> Vec<Item> {
        vec![Item {
            id: ItemId("test:target".into()),
            title: "naobitarget".into(),
            subtitle: String::new(),
            keywords: Vec::new(),
            alias: None,
            kind: ItemKind::App,
            primary: ActionId("open".into()),
            secondary: None,
        }]
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        Vec::new()
    }

    fn run(&self, _id: &ItemId, _action: ActionId, cx: &mut Cx) -> Outcome {
        cx.launch(naobi_core::effect::LaunchTarget::ShellItem(self.0.clone()));
        Outcome::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::db::Db;
    use naobi_core::ui::{Screen, UiEvent};
    use naobi_launcher::frecency::Frecency;
    use naobi_launcher::{Clock, Launcher};

    /// The letters naobi-perf keystroke.rs injects (X, C), each followed by Backspace: queries "x", "", "c", "".
    const LETTERS: [char; 2] = ['x', 'c'];

    struct Zero;
    impl Clock for Zero {
        fn now_ms(&self) -> i64 {
            1_000_000_000_000
        }
    }

    fn engine(n: usize) -> Launcher {
        let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
        let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
        Launcher::new(
            vec![Box::new(PerfSource::new(n))],
            Vec::new(),
            frecency,
            Box::new(Zero),
        )
    }

    fn titles(s: &Screen, n: usize) -> Vec<String> {
        s.sections
            .iter()
            .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
            .take(n)
            .collect()
    }

    fn query(l: &mut Launcher, q: &str) -> Screen {
        l.handle(UiEvent::TextChanged(q.into()));
        l.screen()
    }

    // @R-006
    #[test]
    fn r006_fixture_is_deterministic_and_capped() {
        let (a, b) = (PerfSource::new(10_000), PerfSource::new(10_000));
        assert_eq!(a.items.len(), 10_000);
        assert_eq!(a.items[0].title, "visual terminal 0");
        assert_eq!(a.items, b.items);
        let mut l = engine(10_000);
        assert_eq!(titles(&query(&mut l, "vis"), 100).len(), 50);
    }

    // @R-006
    #[test]
    fn r006_every_injected_key_changes_the_visible_rows() {
        let mut l = engine(10_000);
        // The probe sees the first nine rows (364 px / 40 px).
        let queries = [
            LETTERS[0].to_string(),
            String::new(),
            LETTERS[1].to_string(),
            String::new(),
        ];
        let mut prev = titles(&query(&mut l, &queries[3]), 9);
        for q in queries.iter().cycle().take(8) {
            let now = titles(&query(&mut l, q), 9);
            assert!(!now.is_empty(), "query {q:?} shows no rows");
            assert_ne!(now, prev, "query {q:?} did not change the rows");
            prev = now;
        }
    }
}
