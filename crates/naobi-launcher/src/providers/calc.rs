//! The calculator row (R-014, D-017): an expression typed in the root search shows its result, and
//! Enter copies the result text. A bare number, a word or division by zero shows no row.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use std::sync::Arc;

const ID_PREFIX: &str = "calc:";

pub struct CalcSource {
    now_ms: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl Default for CalcSource {
    fn default() -> Self {
        Self::with_now(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64)
        })
    }
}

impl CalcSource {
    pub fn with_now(now_ms: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            now_ms: Box::new(now_ms),
        }
    }
}

impl CommandSource for CalcSource {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        let Some(out) = naobi_calc::evaluate_at(&q.text, (self.now_ms)()) else {
            return Arc::from(Vec::new());
        };
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
        match id.0.strip_prefix(ID_PREFIX) {
            Some(result) => {
                cx.copy_text(result.to_owned());
                Outcome::Done
            }
            None => Outcome::Done,
        }
    }
}
