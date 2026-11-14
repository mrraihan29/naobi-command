//! Installed apps as root results (R-012, T-003). The catalog is the only source of applications:
//! a file that looks like an app is never listed here. The index worker fills the catalog; until
//! then the source reports itself as loading, so the Launcher shows an "Indexing apps" row.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::LaunchTarget;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::{Arc, RwLock};

/// One installed app. `id` is what follows `shell:AppsFolder\` to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
}

/// The catalog both sides share: the worker `set`s it, the source reads it. Cloning shares it.
#[derive(Clone, Default)]
pub struct AppCatalog(Arc<RwLock<Option<Arc<[Item]>>>>);

pub const ID_PREFIX: &str = "app:";

impl AppCatalog {
    /// Replaces the catalog with a fresh scan. Later scans swap the whole list, so a query in
    /// flight keeps the one it started with.
    pub fn set(&self, apps: Vec<AppEntry>) {
        let mut seen = std::collections::HashSet::new();
        let items: Arc<[Item]> = apps
            .into_iter()
            .filter(|a| seen.insert(a.id.clone()))
            .map(|a| Item {
                id: ItemId(format!("{ID_PREFIX}{}", a.id)),
                title: a.name,
                subtitle: String::new(),
                keywords: Vec::new(),
                alias: None,
                kind: ItemKind::App,
                primary: ActionId("open".into()),
                secondary: None,
            })
            .collect();
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Some(items);
    }

    /// True once a scan has been set.
    pub fn is_ready(&self) -> bool {
        self.snapshot().is_some()
    }

    fn snapshot(&self) -> Option<Arc<[Item]>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub struct AppsSource(AppCatalog);

impl AppsSource {
    pub fn new(catalog: AppCatalog) -> Self {
        Self(catalog)
    }
}

impl CommandSource for AppsSource {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        self.0.snapshot().unwrap_or_else(|| Arc::from(Vec::new()))
    }

    fn pending(&self) -> Option<&'static str> {
        self.0.snapshot().is_none().then_some("Indexing apps")
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
        match id.0.strip_prefix(ID_PREFIX) {
            Some(app) => {
                cx.launch(LaunchTarget::ShellItem(format!(r"shell:AppsFolder\{app}")));
                Outcome::Done
            }
            None => Outcome::Failed(naobi_core::notice::Notice {
                subject: "Open".into(),
                message: "That app is not in the catalog.".into(),
                kind: naobi_core::notice::NoticeKind::Error,
            }),
        }
    }
}
