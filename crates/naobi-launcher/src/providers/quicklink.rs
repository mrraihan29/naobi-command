//! Quicklinks (R-021): `<alias> <text>` opens the quicklink's URL with the text percent-encoded. The
//! quicklinks come from the live settings, so an edit applies after the hot reload.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::LaunchTarget;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::{Notice, NoticeKind};
use naobi_core::quicklink::Quicklink;
use std::sync::{Arc, RwLock};

const ID_PREFIX: &str = "ql:";

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

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![Action {
            id: ActionId("open".into()),
            label: "Open".into(),
            shortcut: None,
            enabled: Ok(()),
        }]
    }

    fn run(&self, id: &ItemId, _action: ActionId, cx: &mut Cx) -> Outcome {
        let url =
            id.0.strip_prefix(ID_PREFIX)
                .and_then(|text| self.0.resolve(text))
                .map(|(_, _, url)| url);
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
