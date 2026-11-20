//! Developer micro-tools through the public `Launcher` engine (Issue #5).
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::dev::{DevEnv, DevSource};
use naobi_launcher::{Clock, Launcher};
use std::sync::{Arc, Mutex};

const T0_MS: i64 = 1_700_000_000_000;

struct FakeClock(i64);
impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

#[derive(Clone, Default)]
struct FakeEnv {
    clipboard: Arc<Mutex<Option<String>>>,
    clipboard_reads: Arc<Mutex<usize>>,
    entropy: Arc<Mutex<Vec<[u8; 16]>>>,
    now_ms: i64,
}

impl FakeEnv {
    fn with_clipboard(text: Option<&str>) -> Self {
        Self {
            clipboard: Arc::new(Mutex::new(text.map(str::to_owned))),
            now_ms: T0_MS,
            ..Self::default()
        }
    }

    fn push_entropy(&self, bytes: [u8; 16]) {
        self.entropy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(bytes);
    }

    fn clipboard_reads(&self) -> usize {
        *self
            .clipboard_reads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

impl DevEnv for FakeEnv {
    fn clipboard_text(&self) -> Option<String> {
        *self
            .clipboard_reads
            .lock()
            .unwrap_or_else(|e| e.into_inner()) += 1;
        self.clipboard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn random16(&self) -> Option<[u8; 16]> {
        let mut q = self.entropy.lock().unwrap_or_else(|e| e.into_inner());
        if q.is_empty() {
            Some([0x11; 16])
        } else {
            Some(q.remove(0))
        }
    }

    fn now_ms(&self) -> i64 {
        self.now_ms
    }
}

fn launcher(env: FakeEnv) -> Launcher {
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, T0_MS / 1000).unwrap_or_else(|e| panic!("frecency: {e}"));
    Launcher::new(
        vec![Box::new(DevSource::new(Box::new(env)))],
        Vec::new(),
        frecency,
        Box::new(FakeClock(T0_MS)),
    )
}

fn type_text(l: &mut Launcher, text: &str) -> Screen {
    let fx = l.handle(UiEvent::TextChanged(text.into()));
    assert!(fx.is_empty());
    l.screen()
}

fn enter(l: &mut Launcher) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key: Key::Enter,
        ctrl: false,
        alt: false,
        shift: false,
    }))
}

fn ctrl_enter(l: &mut Launcher) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key: Key::Enter,
        ctrl: true,
        alt: false,
        shift: false,
    }))
}

#[test]
fn uuid_generates_rfc4122_v4_and_copies_on_enter() {
    let env = FakeEnv::with_clipboard(None);
    // Deliberately set version and variant bits to wrong values to ensure RFC 4122 masking.
    env.push_entropy([
        0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x01, 0xd4, 0x07, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00,
        0x00,
    ]);
    env.push_entropy([0xaa; 16]);
    let mut l = launcher(env.clone());

    let s = type_text(&mut l, "uuid");
    let first = &s.sections[0].rows[0].title;
    assert_eq!(first, "550e8400-e29b-41d4-8716-446655440000");
    assert_eq!(s.sections[0].rows[0].kind, "Utility");
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(first.clone()), Effect::Hide]
    );

    // Second invocation produces a fresh identifier.
    l.show_root();
    let s2 = type_text(&mut l, "uuid");
    assert_eq!(
        s2.sections[0].rows[0].title,
        "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa"
    );
    assert_eq!(env.clipboard_reads(), 0, "uuid must not read clipboard");
}

#[test]
fn ulid_generates_26_char_crockford_base32_and_copies_on_enter() {
    let mut env = FakeEnv::with_clipboard(None);
    env.now_ms = 1_469_918_176_385; // 01ARYZ6S41 in ULID spec
    env.push_entropy([
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0,
    ]);
    let mut l = launcher(env);

    let s = type_text(&mut l, "ulid");
    let id = &s.sections[0].rows[0].title;
    assert_eq!(id.len(), 26);
    assert_eq!(id, "01ARYZ6S41ZZZZZZZZZZZZZZZZ");
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(id.clone()), Effect::Hide]
    );
}

#[test]
fn b64_encodes_and_decodes_inline_and_clipboard_and_rejects_malformed() {
    let env = FakeEnv::with_clipboard(Some("hello world"));
    let mut l = launcher(env.clone());

    let s = type_text(&mut l, "b64 foobar");
    assert_eq!(s.sections[0].rows[0].title, "Zm9vYmFy");
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText("Zm9vYmFy".into()), Effect::Hide]
    );

    l.show_root();
    assert_eq!(
        type_text(&mut l, "b64 encode f").sections[0].rows[0].title,
        "Zg=="
    );
    assert_eq!(
        type_text(&mut l, "b64d Zm9vYmFy").sections[0].rows[0].title,
        "foobar"
    );
    assert_eq!(
        type_text(&mut l, "b64 decode Zm8=").sections[0].rows[0].title,
        "fo"
    );
    assert_eq!(env.clipboard_reads(), 0, "inline args skip clipboard");

    // Clipboard fallback when argument omitted.
    let s = type_text(&mut l, "b64");
    assert_eq!(s.sections[0].rows[0].title, "aGVsbG8gd29ybGQ=");
    assert_eq!(env.clipboard_reads(), 1);

    *env.clipboard.lock().unwrap_or_else(|e| e.into_inner()) = Some("aGVsbG8=".into());
    assert_eq!(type_text(&mut l, "b64d").sections[0].rows[0].title, "hello");
    assert_eq!(
        type_text(&mut l, "b64 decode").sections[0].rows[0].title,
        "hello"
    );

    // Malformed Base64 produces a clear non-crashing Notice row.
    for bad in ["b64d !!!", "b64d a", "b64d ====", "b64d //4="] {
        let s = type_text(&mut l, bad);
        assert!(
            !s.sections.is_empty() && !s.sections[0].rows.is_empty(),
            "{bad} must show a row"
        );
        let fx = enter(&mut l);
        assert!(
            matches!(fx.as_slice(), [Effect::Notice(_)]),
            "{bad} Enter should emit Notice without CopyText or Hide: {fx:?}"
        );
    }
}

#[test]
fn url_encodes_and_decodes_inline_and_clipboard_and_rejects_malformed() {
    let env = FakeEnv::with_clipboard(Some("a b&c=100%"));
    let mut l = launcher(env.clone());

    let s = type_text(&mut l, "url hello world?x=1&y=2");
    assert_eq!(s.sections[0].rows[0].title, "hello%20world%3Fx%3D1%26y%3D2");
    assert_eq!(
        enter(&mut l),
        vec![
            Effect::CopyText("hello%20world%3Fx%3D1%26y%3D2".into()),
            Effect::Hide
        ]
    );

    l.show_root();
    assert_eq!(
        type_text(&mut l, "urld hello%20world%3Fx%3D1").sections[0].rows[0].title,
        "hello world?x=1"
    );
    assert_eq!(
        type_text(&mut l, "url decode react%2Bnative").sections[0].rows[0].title,
        "react+native"
    );
    assert_eq!(
        type_text(&mut l, "url").sections[0].rows[0].title,
        "a%20b%26c%3D100%25"
    );

    *env.clipboard.lock().unwrap_or_else(|e| e.into_inner()) = Some("foo%20bar".into());
    assert_eq!(
        type_text(&mut l, "urld").sections[0].rows[0].title,
        "foo bar"
    );

    for bad in ["urld %ZZ", "urld 100%", "urld %2", "urld %FF%FE"] {
        let s = type_text(&mut l, bad);
        assert!(!s.sections.is_empty(), "{bad} must show an error row");
        let fx = enter(&mut l);
        assert!(
            matches!(fx.as_slice(), [Effect::Notice(_)]),
            "{bad} Enter should emit Notice: {fx:?}"
        );
    }
}

#[test]
fn sha256_hashes_inline_and_clipboard_text() {
    let env = FakeEnv::with_clipboard(Some(
        "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
    ));
    let mut l = launcher(env);

    let s = type_text(&mut l, "sha256 abc");
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(s.sections[0].rows[0].title, expected);
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(expected.into()), Effect::Hide]
    );

    l.show_root();
    let s2 = type_text(&mut l, "sha256");
    let expected_multi = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
    assert_eq!(s2.sections[0].rows[0].title, expected_multi);
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(expected_multi.into()), Effect::Hide]
    );
}

#[test]
fn json_formats_minifies_inline_and_clipboard_and_rejects_invalid() {
    let env = FakeEnv::with_clipboard(Some("{\n  \"x\" : [ 1 , 2 ]\n}"));
    let mut l = launcher(env.clone());

    let s = type_text(&mut l, r#"json {"b":1,"a":[true,null,"hi"]}"#);
    assert!(!s.sections[0].rows[0].title.is_empty());
    let expected_pretty = "{\n  \"b\": 1,\n  \"a\": [\n    true,\n    null,\n    \"hi\"\n  ]\n}";
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(expected_pretty.into()), Effect::Hide]
    );

    // Secondary action (Ctrl+Enter) copies minified JSON.
    l.show_root();
    type_text(&mut l, r#"json {"b":1,"a":[true,null,"hi"]}"#);
    assert_eq!(
        ctrl_enter(&mut l),
        vec![
            Effect::CopyText(r#"{"b":1,"a":[true,null,"hi"]}"#.into()),
            Effect::Hide
        ]
    );

    // Explicit `json minify` / `json min` and `json fmt` with clipboard fallback.
    l.show_root();
    let s_min = type_text(&mut l, "json min");
    assert_eq!(s_min.sections[0].rows[0].title, r#"{"x":[1,2]}"#);
    assert_eq!(
        enter(&mut l),
        vec![Effect::CopyText(r#"{"x":[1,2]}"#.into()), Effect::Hide]
    );

    l.show_root();
    type_text(&mut l, "json");
    assert_eq!(
        enter(&mut l),
        vec![
            Effect::CopyText("{\n  \"x\": [\n    1,\n    2\n  ]\n}".into()),
            Effect::Hide
        ]
    );

    // Invalid JSON produces a clear non-crashing Notice row.
    for bad in [
        "json {bad",
        "json [1,]",
        r#"json {"a":01}"#,
        r#"json {"a":1} extra"#,
        "json \"unterminated",
    ] {
        let s = type_text(&mut l, bad);
        assert!(!s.sections.is_empty(), "{bad} must show an error row");
        let fx = enter(&mut l);
        assert!(
            matches!(fx.as_slice(), [Effect::Notice(_)]),
            "{bad} Enter should emit Notice: {fx:?}"
        );
    }
}

#[test]
fn wc_counts_words_chars_lines_inline_and_from_clipboard() {
    let env = FakeEnv::with_clipboard(Some("alpha beta\ngamma"));
    let mut l = launcher(env);

    let s = type_text(&mut l, "wc hello world");
    assert_eq!(s.sections[0].rows[0].title, "2 words, 11 chars, 1 line");
    assert_eq!(
        enter(&mut l),
        vec![
            Effect::CopyText("2 words, 11 chars, 1 line".into()),
            Effect::Hide
        ]
    );

    l.show_root();
    assert_eq!(
        type_text(&mut l, "wc x").sections[0].rows[0].title,
        "1 word, 1 char, 1 line"
    );

    let s_clip = type_text(&mut l, "wc");
    assert_eq!(
        s_clip.sections[0].rows[0].title,
        "3 words, 16 chars, 2 lines"
    );
    assert_eq!(
        enter(&mut l),
        vec![
            Effect::CopyText("3 words, 16 chars, 2 lines".into()),
            Effect::Hide
        ]
    );
}

#[test]
fn empty_or_missing_clipboard_produces_notice_and_ephemeral_skips_frecency() {
    for clip in [None, Some("")] {
        let env = FakeEnv::with_clipboard(clip);
        let mut l = launcher(env);
        for cmd in ["b64", "b64d", "url", "urld", "sha256", "json", "wc"] {
            let s = type_text(&mut l, cmd);
            assert!(!s.sections.is_empty(), "{cmd} should show a notice row");
            let fx = enter(&mut l);
            assert!(
                matches!(fx.as_slice(), [Effect::Notice(_)]),
                "{cmd} with empty clipboard should emit Notice: {fx:?}"
            );
        }
    }

    // Secret clipboard payload must not appear in error row or Notice on malformed input.
    let secret = "zq-secret-clip-4910!";
    let env = FakeEnv::with_clipboard(Some(secret));
    let mut l = launcher(env);
    for cmd in ["b64d", "json"] {
        let s = type_text(&mut l, cmd);
        let row = &s.sections[0].rows[0];
        assert!(!row.title.contains(secret));
        assert!(!row.subtitle.contains(secret));
        let fx = enter(&mut l);
        let [Effect::Notice(n)] = fx.as_slice() else {
            panic!("expected Notice for {cmd}: {fx:?}");
        };
        assert!(!n.subject.contains(secret) && !n.message.contains(secret));
    }

    // Dev tool runs are ephemeral: empty query must not surface them as frecent items.
    l.show_root();
    type_text(&mut l, "uuid");
    let fx = enter(&mut l);
    assert!(matches!(fx.as_slice(), [Effect::CopyText(_), Effect::Hide]));
    l.show_root();
    assert!(l.screen().sections.is_empty());
}
