//! Ranking (R-012, D-016): exact alias first, then the nucleo fuzzy score plus a frecency boost
//! capped at 20% of that score; ties go to more opens, then title.
use naobi_core::ids::ItemId;
use naobi_core::item::Item;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::cmp::Ordering;

/// Rows the root search shows (REPORT: at most 50).
pub const MAX_ROWS: usize = 50;
/// Rows the empty query lists, the most frecent items (D-016).
pub const EMPTY_QUERY_ROWS: usize = 10;
/// File rows kept in the trailing section (D-016).
pub const MAX_FILE_ROWS: usize = 30;
/// Weak-match floor for queries of three or more characters (D-016). Three-letter matches in a
/// word score 60 or more; on a synthetic 10,000-title set nothing useful scores between 55 and 62.
/// Not yet tuned on the real catalog (see coder-ncmd14.md).
pub const MIN_FUZZY: u32 = 60;
/// A frecency boost is worth 5 points per decayed open (min with the cap below).
const BOOST_PER_OPEN: f64 = 5.0;
/// The boost never exceeds this share of the fuzzy score (R-012 v2).
const BOOST_CAP: f64 = 0.2;

/// The fuzzy floor for a query of `chars` characters (D-016). One or two characters have no score
/// floor: any real subsequence match is a row (the engine shows the best 50), and a title with no
/// match is never a row. Round 2 chose this after a floor of 60 returned nothing for "vs", "wt",
/// "pt" and "pw" on a 10,000-title set (a floor of 50 did too). Three characters and up use
/// `MIN_FUZZY`, so weak scattered matches stay out. Not tuned on the real catalog (NCMD-16).
pub fn floor_for(chars: usize) -> u32 {
    if chars <= 2 {
        0
    } else {
        MIN_FUZZY
    }
}

/// True when a fuzzy score clears the floor for a query of `chars` characters.
pub fn passes_floor(fuzzy: u32, chars: usize) -> bool {
    fuzzy >= floor_for(chars)
}

/// Frecency boost: `min(5 x opens, 20% of the fuzzy score)` (R-012 v2).
pub fn boost(fuzzy: f64, opens: f64) -> f64 {
    (BOOST_PER_OPEN * opens).min(BOOST_CAP * fuzzy)
}

/// Which command source an item came from, so the engine can run its actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Index into the root sources.
    Root(usize),
    /// Index into the hosted views.
    View(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub src: Source,
    pub item: Item,
    /// The query is this item's alias.
    pub exact: bool,
    /// Fuzzy score plus the capped boost; the decayed open count when the query is empty.
    pub total: f64,
    pub opens: f64,
}

impl Hit {
    pub fn id(&self) -> &ItemId {
        &self.item.id
    }
}

fn order(a: &Hit, b: &Hit) -> Ordering {
    b.exact
        .cmp(&a.exact)
        .then_with(|| b.total.total_cmp(&a.total))
        .then_with(|| b.opens.total_cmp(&a.opens))
        .then_with(|| a.item.title.cmp(&b.item.title))
}

/// Reuses one matcher and one buffer across keystrokes, so ranking allocates per query, not per item.
pub struct Ranker {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Default for Ranker {
    fn default() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
        }
    }
}

impl Ranker {
    /// The best `limit` hits for `text` (trimmed), in display order. Empty `text` keeps everything
    /// and orders by `opens` (the most frecent first).
    pub fn rank(
        &mut self,
        text: &str,
        pool: Vec<(Source, Item)>,
        opens: impl Fn(&ItemId) -> f64,
        limit: usize,
    ) -> Vec<Hit> {
        let pat = (!text.is_empty())
            .then(|| Pattern::parse(text, CaseMatching::Ignore, Normalization::Smart));
        let chars = text.chars().count();
        let mut hits = Vec::with_capacity(pool.len().min(limit * 2));
        for (src, item) in pool {
            let Some(pat) = &pat else {
                let opens = opens(&item.id);
                hits.push(Hit {
                    src,
                    item,
                    exact: false,
                    total: opens,
                    opens,
                });
                continue;
            };
            let exact = item
                .alias
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(text));
            // A row needs a real subsequence match, whatever the query length. The score floor only
            // applies to matches that exist; an alias hit is always a row.
            let fuzzy = match self.best(pat, &item) {
                Some(f) => f,
                None if exact => 0,
                None => continue,
            };
            if !exact && !passes_floor(fuzzy, chars) {
                continue;
            }
            let opens = opens(&item.id);
            let f = f64::from(fuzzy);
            let total = f + boost(f, opens);
            hits.push(Hit {
                src,
                item,
                exact,
                total,
                opens,
            });
        }
        if hits.len() > limit {
            hits.select_nth_unstable_by(limit, order);
            hits.truncate(limit);
        }
        hits.sort_by(order);
        hits
    }

    /// The better of the title and any keyword.
    fn best(&mut self, pat: &Pattern, item: &Item) -> Option<u32> {
        let mut best = pat.score(Utf32Str::new(&item.title, &mut self.buf), &mut self.matcher);
        for kw in &item.keywords {
            let s = pat.score(Utf32Str::new(kw, &mut self.buf), &mut self.matcher);
            best = best.max(s);
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naobi_core::ids::ActionId;
    use naobi_core::item::ItemKind;

    fn pooled(i: usize) -> (Source, Item) {
        (
            Source::Root(0),
            Item {
                id: ItemId(format!("app:{i}")),
                title: format!("note {i}"),
                subtitle: String::new(),
                keywords: vec![],
                alias: None,
                kind: ItemKind::App,
                primary: ActionId("open".into()),
                secondary: None,
            },
        )
    }

    #[test]
    fn floor_is_inclusive_at_three_letters() {
        assert!(passes_floor(60, 3));
        assert!(!passes_floor(59, 3));
        assert_eq!(floor_for(3), MIN_FUZZY);
    }

    #[test]
    fn one_and_two_letter_queries_have_no_floor() {
        assert!(passes_floor(0, 1));
        assert!(passes_floor(0, 2));
        assert_eq!(floor_for(1), 0);
        assert_eq!(floor_for(2), 0);
    }

    #[test]
    fn boost_is_five_points_per_open_below_the_cap() {
        assert_eq!(boost(100.0, 2.0), 10.0);
        assert_eq!(boost(100.0, 3.0), 15.0);
    }

    #[test]
    fn boost_is_capped_at_twenty_percent_of_the_fuzzy_score() {
        assert_eq!(boost(100.0, 50.0), 20.0);
        assert_eq!(boost(10.0, 2.0), 2.0);
    }

    #[test]
    fn rank_keeps_exactly_limit_rows() {
        let pool: Vec<_> = (0..10).map(pooled).collect();
        assert_eq!(Ranker::default().rank("", pool, |_| 0.0, 10).len(), 10);
    }

    #[test]
    fn rank_truncates_one_row_over_limit() {
        let pool: Vec<_> = (0..11).map(pooled).collect();
        assert_eq!(Ranker::default().rank("", pool, |_| 0.0, 10).len(), 10);
    }
}
