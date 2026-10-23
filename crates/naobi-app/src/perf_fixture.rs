//! Synthetic index for the latency item (feature `perf`): deterministic titles matched with nucleo,
//! top 50 by a bounded heap. Not a ranking: ties break by index, nothing is frecency-weighted.
use naobi_core::ui::Row;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

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

/// Rows the Launcher keeps (REPORT: at most 50).
const TOP: usize = 50;

pub struct Fixture {
    items: Vec<Row>,
    matcher: Matcher,
    buf: Vec<char>,
}

impl Fixture {
    pub fn new(n: usize) -> Self {
        let items = (0..n)
            .map(|i| {
                let (a, b) = (WORDS[i % 64], WORDS[(i * 7 + 3) % 64]);
                Row {
                    title: format!("{a} {b} {i}"),
                    subtitle: format!("C:\\Program Files\\{a}\\{b}.exe"),
                }
            })
            .collect();
        Fixture {
            items,
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
        }
    }

    /// Top 50 rows for `q`: score descending, index ascending. An empty query lists the first 50.
    pub fn top(&mut self, q: &str) -> Vec<Row> {
        if q.is_empty() {
            return self.items.iter().take(TOP).cloned().collect();
        }
        let pat = Pattern::parse(q, CaseMatching::Ignore, Normalization::Smart);
        // Min-heap on (score, -index): the root is the weakest kept hit.
        let mut heap = BinaryHeap::with_capacity(TOP + 1);
        for (i, it) in self.items.iter().enumerate() {
            let hay = Utf32Str::new(&it.title, &mut self.buf);
            if let Some(s) = pat.score(hay, &mut self.matcher) {
                heap.push(Reverse((s, Reverse(i))));
                if heap.len() > TOP {
                    heap.pop();
                }
            }
        }
        let mut hits: Vec<_> = heap
            .into_iter()
            .map(|Reverse((s, Reverse(i)))| (s, i))
            .collect();
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        hits.into_iter()
            .map(|(_, i)| self.items[i].clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The letters naobi-perf keystroke.rs injects (X, C), each followed by Backspace: queries "x", "", "c", "".
    const LETTERS: [char; 2] = ['x', 'c'];

    fn titles(f: &mut Fixture, q: &str, n: usize) -> Vec<String> {
        f.top(q).into_iter().take(n).map(|r| r.title).collect()
    }

    // @R-006
    #[test]
    fn r006_fixture_is_deterministic_and_capped() {
        let (mut a, mut b) = (Fixture::new(10_000), Fixture::new(10_000));
        assert_eq!(a.items.len(), 10_000);
        assert_eq!(a.items[0].title, "visual terminal 0");
        assert_eq!(a.top("vis"), b.top("vis"));
        assert_eq!(a.top("vis").len(), 50);
        assert_eq!(a.top("").len(), 50);
    }

    // @R-006
    #[test]
    fn r006_every_injected_key_changes_the_visible_rows() {
        let mut f = Fixture::new(10_000);
        // The probe sees the first nine rows (364 px / 40 px).
        let queries = [
            LETTERS[0].to_string(),
            String::new(),
            LETTERS[1].to_string(),
            String::new(),
        ];
        let mut prev = titles(&mut f, &queries[3], 9);
        for q in queries.iter().cycle().take(8) {
            let now = titles(&mut f, q, 9);
            assert_ne!(now, prev, "query {q:?} did not change the rows");
            prev = now;
        }
    }
}
