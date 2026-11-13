//! Compile-level check: a module crate can provide a view using core only.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::Arc;

struct FakeView;

impl CommandSource for FakeView {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        Arc::from(Vec::new())
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![]
    }
    fn run(&self, _id: &ItemId, _action: ActionId, cx: &mut Cx) -> Outcome {
        cx.hide();
        Outcome::Effects(cx.take())
    }
}

impl ViewSource for FakeView {
    fn view_id(&self) -> ViewId {
        ViewId("fake".into())
    }
    fn opened_by_hotkey(&self) -> bool {
        true
    }
    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        Some(Preview::Text("x".into()))
    }
}

#[test]
fn fake_view_is_a_view_source() {
    let v: Box<dyn ViewSource> = Box::new(FakeView);
    assert_eq!(v.view_id(), ViewId("fake".into()));
    assert!(v.opened_by_hotkey());
    assert!(matches!(
        v.run(&ItemId("a".into()), ActionId("b".into()), &mut Cx::default()),
        Outcome::Effects(e) if e.len() == 1
    ));
}
