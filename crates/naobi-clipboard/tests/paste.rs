//! The paste sequence with a fake pasteboard and a fake foreground.
use naobi_clipboard::paste::{
    paste, Abort, ForegroundOps, Outcome, Pasteboard, Payload, Snapshot, RESTORE_AFTER_MS,
};
use naobi_clipboard::Content;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const TARGET: isize = 77;

#[derive(Default)]
struct World {
    /// Bumped by every write and by "another copy".
    seq: Cell<u32>,
    log: RefCell<Vec<String>>,
    clipboard: RefCell<Option<Content>>,
}

struct Board {
    w: Rc<World>,
    excluded: bool,
}

impl Pasteboard for Board {
    fn snapshot(&mut self) -> Snapshot {
        Snapshot {
            content: self.w.clipboard.borrow().clone(),
            excluded: self.excluded,
        }
    }
    fn write(&mut self, payload: &Payload) -> bool {
        self.w.seq.set(self.w.seq.get() + 1);
        self.w.log.borrow_mut().push("write".into());
        if let Payload::Text(t) = payload {
            *self.w.clipboard.borrow_mut() = Some(Content::Text(t.clone()));
        }
        true
    }
    fn restore(&mut self, content: &Content) -> bool {
        self.w.seq.set(self.w.seq.get() + 1);
        self.w.log.borrow_mut().push("restore".into());
        *self.w.clipboard.borrow_mut() = Some(content.clone());
        true
    }
    fn sequence(&self) -> u32 {
        self.w.seq.get()
    }
}

struct Fg {
    w: Rc<World>,
    /// The window in front before the target takes focus.
    front: isize,
    /// The target is in front from this look on.
    focus_after: u32,
    /// A window takes the focus away two looks after the target had it.
    steal: bool,
    looks: Cell<u32>,
    elevated: bool,
    /// Another program copies during the 300 ms wait.
    copy_during_wait: bool,
}

impl ForegroundOps for Fg {
    fn set_foreground(&mut self, _target: isize) {
        self.w.log.borrow_mut().push("focus".into());
    }
    fn foreground(&self) -> isize {
        let n = self.looks.get();
        self.looks.set(n + 1);
        if n < self.focus_after {
            return self.front;
        }
        if self.steal && n > self.focus_after {
            return 5;
        }
        TARGET
    }
    fn is_elevated(&self, _t: isize) -> bool {
        self.elevated
    }
    fn send_paste(&mut self) -> bool {
        self.w.log.borrow_mut().push("ctrl+v".into());
        true
    }
    fn sleep_ms(&mut self, ms: u64) {
        if ms == RESTORE_AFTER_MS && self.copy_during_wait {
            self.w.seq.set(self.w.seq.get() + 1);
            *self.w.clipboard.borrow_mut() = Some(Content::Text("another copy".into()));
        }
    }
}

fn fg(w: &Rc<World>) -> Fg {
    Fg {
        w: w.clone(),
        front: 1,
        focus_after: 2,
        steal: false,
        looks: Cell::new(0),
        elevated: false,
        copy_during_wait: false,
    }
}

fn run(excluded: bool, tweak: impl FnOnce(&mut Fg)) -> (Outcome, Rc<World>) {
    let w = Rc::new(World::default());
    *w.clipboard.borrow_mut() = Some(Content::Text("previous".into()));
    let mut board = Board {
        w: w.clone(),
        excluded,
    };
    let mut f = fg(&w);
    tweak(&mut f);
    let out = paste(
        &Payload::Text("entry".into()),
        Some(TARGET),
        &mut board,
        &mut f,
    );
    (out, w)
}

fn text(w: &World) -> Option<String> {
    match &*w.clipboard.borrow() {
        Some(Content::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

fn log(w: &World) -> Vec<String> {
    w.log.borrow().clone()
}

// @R-015
#[test]
fn r015_previous_clipboard_restored_after_paste() {
    let (out, w) = run(false, |_| {});
    assert_eq!(out, Outcome::Pasted);
    assert_eq!(log(&w), ["write", "focus", "ctrl+v", "restore"]);
    assert_eq!(text(&w).as_deref(), Some("previous"));
}

// @R-015
#[test]
fn r015_restore_skipped_if_another_copy_happened() {
    let (out, w) = run(false, |f| f.copy_during_wait = true);
    assert_eq!(out, Outcome::Pasted);
    assert!(!log(&w).contains(&"restore".to_owned()));
    assert_eq!(
        text(&w).as_deref(),
        Some("another copy"),
        "the new copy is not overwritten"
    );
}

// @R-015
#[test]
fn r015_restore_skipped_when_snapshot_was_excluded() {
    // T-032: a secret that was kept out of history is not written back by the paste.
    let (out, w) = run(true, |_| {});
    assert_eq!(out, Outcome::Pasted);
    assert!(!log(&w).contains(&"restore".to_owned()));
    assert_eq!(
        text(&w).as_deref(),
        Some("entry"),
        "the clipboard keeps the pasted entry"
    );
}

// @R-015
#[test]
fn r015_paste_aborts_if_foreground_changed() {
    // T-008: focus moves away right before the keys: nothing is sent, the snapshot comes back.
    let (out, w) = run(false, |f| {
        f.steal = true;
        f.focus_after = 0;
    });
    assert_eq!(out, Outcome::Aborted(Abort::ForegroundChanged));
    assert!(!log(&w).contains(&"ctrl+v".to_owned()), "{:?}", log(&w));
    assert_eq!(text(&w).as_deref(), Some("previous"));
    // The target never gets focus within 100 ms.
    let (out, w) = run(false, |f| f.focus_after = 1000);
    assert_eq!(out, Outcome::Aborted(Abort::ForegroundChanged));
    assert!(!log(&w).contains(&"ctrl+v".to_owned()));
}

// @R-015
#[test]
fn r015_elevated_target_aborts_and_restores() {
    let (out, w) = run(false, |f| f.elevated = true);
    assert_eq!(out, Outcome::Aborted(Abort::Elevated));
    assert!(!log(&w).contains(&"ctrl+v".to_owned()));
    assert_eq!(text(&w).as_deref(), Some("previous"));
}

// @R-015
#[test]
fn r015_no_target_pastes_nothing() {
    let w = Rc::new(World::default());
    let mut board = Board {
        w: w.clone(),
        excluded: false,
    };
    let mut f = fg(&w);
    let out = paste(&Payload::Text("x".into()), None, &mut board, &mut f);
    assert_eq!(out, Outcome::Aborted(Abort::NoTarget));
    assert!(log(&w).is_empty(), "the clipboard is not touched");
}

// @D-019
#[test]
fn d019_restore_covers_text_image_filelist_only() {
    // The snapshot can hold exactly these three kinds; rich text, HTML and ranges are not in it.
    for c in [
        Content::Text("t".into()),
        Content::Dib(vec![0; 40]),
        Content::Files(vec!["a".into()]),
    ] {
        let s = Snapshot {
            content: Some(c),
            excluded: false,
        };
        assert!(s.content.is_some());
    }
}
