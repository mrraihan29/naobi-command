//! Quicklinks (R-021): `<alias> <text>` opens the quicklink's URL with the text percent-encoded. The
//! quicklinks come from the live settings, so an edit applies after the hot reload. Text whose first
//! word is no alias gets one last fallback row built from the `g` quicklink (R-034).
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::LaunchTarget;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::quicklink::Quicklink;
use std::sync::{Arc, RwLock};

const ID_PREFIX: &str = "ql:";
/// Fallback rows (R-034) carry the whole text, not an alias, so `run` resolves them apart.
const FALLBACK_PREFIX: &str = "qf:";
/// The default quicklink of D-024 that the fallback row uses; editable in the settings.
const FALLBACK_ALIAS: &str = "g";

/// The quicklinks both sides share: the settings reload `set`s them, the source reads them.
#[derive(Clone, Default)]
pub struct QuicklinkSet(Arc<RwLock<Vec<Quicklink>>>);

impl QuicklinkSet {
    pub fn set(&self, links: Vec<Quicklink>) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = links;
    }

    /// The quicklink for `text` (`<alias> <rest>`, the alias compared without case), the search text
    /// and the URL built from it.
    fn resolve(&self, text: &str) -> Option<(Quicklink, String, String)> {
        let (alias, rest) = text.split_once(char::is_whitespace)?;
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }
        let links = self.0.read().unwrap_or_else(|e| e.into_inner());
        let link = links.iter().find(|l| l.alias.eq_ignore_ascii_case(alias))?;
        let url = link.build_url(rest).ok()?;
        Some((link.clone(), rest.to_owned(), url))
    }

    /// The `g` quicklink and the URL for the whole `text` (R-034), or None when the first word is
    /// any alias, there is no `g`, or the URL cannot be built.
    fn fallback(&self, text: &str) -> Option<(Quicklink, String, String)> {
        let text = text.trim();
        let first = text.split_whitespace().next()?;
        let links = self.0.read().unwrap_or_else(|e| e.into_inner());
        if links.iter().any(|l| l.alias.eq_ignore_ascii_case(first)) {
            return None;
        }
        let link = links
            .iter()
            .find(|l| l.alias.eq_ignore_ascii_case(FALLBACK_ALIAS))?;
        let url = link.build_url(text).ok()?;
        Some((link.clone(), text.to_owned(), url))
    }
}

pub struct QuicklinkSource(QuicklinkSet);

impl QuicklinkSource {
    pub fn new(set: QuicklinkSet) -> Self {
        Self(set)
    }
}

impl CommandSource for QuicklinkSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let Some((link, rest, url)) = self.0.resolve(&q.text) else {
            return Arc::from(Vec::new());
        };
        vec![Item {
            id: ItemId(format!("{ID_PREFIX}{}", q.text)),
            title: format!("Search {} for {rest}", link.name),
            subtitle: url,
            keywords: Vec::new(),
            // The whole query is this row's alias, so ranking always keeps it and puts it first.
            alias: Some(q.text.clone()),
            kind: ItemKind::Quicklink,
            primary: ActionId("open".into()),
            secondary: None,
        }]
        .into()
    }

    fn fallback(&self, q: &Query) -> Option<Item> {
        let (link, text, url) = self.0.fallback(&q.text)?;
        Some(Item {
            id: ItemId(format!("{FALLBACK_PREFIX}{text}")),
            title: format!("Search {} for {text}", link.name),
            subtitle: url,
            keywords: Vec::new(),
            // No alias: the engine lists it last without ranking, so no query can drop or reorder it.
            alias: None,
            kind: ItemKind::Quicklink,
            primary: ActionId("open".into()),
            secondary: None,
        })
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("open".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, cx: &mut Cx) -> Outcome {
        let url = match id.0.strip_prefix(FALLBACK_PREFIX) {
            Some(text) => self.0.fallback(text).map(|(_, _, url)| url),
            None => {
                id.0.strip_prefix(ID_PREFIX)
                    .and_then(|text| self.0.resolve(text))
                    .map(|(_, _, url)| url)
            }
        };
        match url {
            Some(url) => {
                cx.launch(LaunchTarget::Url(url));
                Outcome::Done
            }
            // The quicklink was edited or removed between the row and the Enter.
            None => Outcome::Failed(Notice {
                subject: "Quicklink".into(),
                message: "That quicklink is no longer defined.".into(),
                kind: NoticeKind::Error,
            }),
        }
    }
}
