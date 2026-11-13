//! The clipboard history through its public API, with a tagged cipher and a fake clock.
use naobi_clipboard::capture::{capture, Capture};
use naobi_clipboard::filters::Skip;
use naobi_clipboard::history::Outcome;
use naobi_clipboard::limits::{Limiter, Limits, INDEX_TTL_MS, MAX_TEXT_BYTES};
use naobi_clipboard::testing::{FakeClock, TaggedCipher};
use naobi_clipboard::{Captured, ClipSource, Content, Error, Flags, History, Purged, Stored};
use naobi_core::db::Db;
use std::cell::Cell;
use zeroize::Zeroizing;

const DAY_MS: i64 = 86_400_000;

fn history_with(limits: Limits) -> (History, FakeClock) {
    let clock = FakeClock::at(1_700_000_000_000);
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let h = History::new(db, Box::new(TaggedCipher), Box::new(clock.clone())).with_limits(limits);
    (h, clock)
}

fn history() -> (History, FakeClock) {
    history_with(Limits::default())
}

fn cap(content: Content) -> Captured {
    Captured {
        flags: Flags::default(),
        marker: false,
        owner_exe: Some("notepad.exe".into()),
        content,
    }
}

fn text(t: &str) -> Captured {
    cap(Content::Text(t.into()))
}

fn store(h: &mut History, c: Captured) -> Outcome {
    h.on_change(c).unwrap_or_else(|e| panic!("on_change: {e}"))
}

fn id_of(o: &Outcome) -> i64 {
    match o.result {
        Ok(Stored::New(id) | Stored::Refreshed(id)) => id,
        Err(skip) => panic!("skipped: {skip:?}"),
    }
}

fn count(h: &History) -> usize {
    h.list(10_000).map(|l| l.len()).unwrap_or(0)
}

/// A 24-bit bottom-up bitmap of `w` x `h`, filled by `f(index)`.
fn dib(w: u32, h: u32, f: impl FnMut(usize) -> u8) -> Vec<u8> {
    let stride = (w as usize * 3).div_ceil(4) * 4;
    let mut v = Vec::new();
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&24u16.to_le_bytes());
    v.extend_from_slice(&[0; 24]);
    v.extend((0..stride * h as usize).map(f));
    v
}

// @R-009
#[test]
fn r009_stored_content_round_trips_through_the_cipher() {
    let (mut h, _) = history();
    let id = id_of(&store(&mut h, text("zq-secret-marker-5521")));
    let preview = h.preview(id).ok().flatten();
    assert_eq!(
        preview.as_deref().map(String::as_str),
        Some("zq-secret-marker-5521")
    );
    let payload = h.payload(id).ok().flatten();
    assert_eq!(
        payload.as_deref().map(Vec::as_slice),
        Some(&b"zq-secret-marker-5521"[..])
    );
    // Only the cipher's output is stored: a blob that is not tagged does not decrypt.
    let listed = h.list(10).unwrap_or_default();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].source_exe.as_deref(), Some("notepad.exe"));
}

// @R-009
#[test]
fn r009_retention_purges_old_and_excess() {
    let (mut h, clock) = history_with(Limits {
        max_entries: 3,
        ..Limits::default()
    });
    for i in 0..5 {
        store(&mut h, text(&format!("entry {i}")));
        clock.advance(1000);
    }
    assert_eq!(count(&h), 3, "beyond the cap the oldest go");
    // 31 days later one new copy purges everything older than 30 days.
    clock.advance(31 * DAY_MS);
    let o = store(&mut h, text("fresh"));
    assert_eq!(
        o.purged,
        Purged {
            expired: 3,
            excess: 0,
            evicted: 0
        }
    );
    assert_eq!(count(&h), 1);
}

// @R-009
#[test]
fn r009_exclusion_flag_not_stored() {
    let (mut h, _) = history();
    let flagged = [
        Flags {
            exclude_from_monitor: true,
            ..Flags::default()
        },
        Flags {
            can_include_in_history: Some(0),
            ..Flags::default()
        },
        Flags {
            can_upload_to_cloud: Some(0),
            ..Flags::default()
        },
    ];
    for flags in flagged {
        let mut c = text("secret");
        c.flags = flags;
        assert_eq!(store(&mut h, c).result, Err(Skip::Excluded), "{flags:?}");
    }
    // The permissive values do not exclude.
    let mut c = text("fine");
    c.flags.can_include_in_history = Some(1);
    c.flags.can_upload_to_cloud = Some(1);
    assert!(store(&mut h, c).result.is_ok());
    assert_eq!(count(&h), 1);
}

// @R-009
#[test]
fn r009_clear_history_leaves_zero() {
    let (mut h, _) = history();
    let a = id_of(&store(&mut h, text("one")));
    store(&mut h, text("two"));
    assert!(h.pin(a).is_ok());
    assert_eq!(h.clear().ok(), Some(2));
    assert_eq!(count(&h), 0, "pinned entries go too");
    assert_eq!(h.payload(a).ok().flatten(), None, "payload rows cascade");
}

// @R-015
#[test]
fn r015_image_over_10mb_rejected() {
    let (mut h, _) = history();
    // 2000 x 2000 x 3 bytes of noise: 12 MB raw, and PNG cannot shrink noise below 10 MB.
    let mut x = 0x2545_F491u32;
    let noise = dib(2000, 2000, |_| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x >> 8) as u8
    });
    assert_eq!(
        store(&mut h, cap(Content::Dib(noise))).result,
        Err(Skip::TooLarge)
    );
    // A small bitmap is stored as a PNG.
    let small = dib(8, 8, |i| (i * 7) as u8);
    let id = id_of(&store(&mut h, cap(Content::Dib(small))));
    let png = h.payload(id).ok().flatten().unwrap_or_default();
    assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    assert_eq!(
        h.preview(id).ok().flatten().as_deref().map(String::as_str),
        Some("Image 8 x 8")
    );
}

// @D-015
#[test]
fn d015_oversize_bitmap_rejected_before_conversion() {
    // Not even a valid header: the size check must answer first.
    let raw = vec![0u8; 64 * 1024 * 1024 + 1];
    assert_eq!(
        naobi_clipboard::image::dib_to_png(&raw),
        Err(naobi_clipboard::image::ImageError::TooLargeBitmap)
    );
    let (mut h, _) = history();
    assert_eq!(
        store(&mut h, cap(Content::Dib(raw))).result,
        Err(Skip::TooLarge)
    );
}

// @R-015
#[test]
fn r015_pinned_entry_survives_retention_purge() {
    let (mut h, clock) = history_with(Limits {
        max_entries: 1,
        ..Limits::default()
    });
    let keep = id_of(&store(&mut h, text("pinned one")));
    assert!(h.pin(keep).is_ok());
    clock.advance(40 * DAY_MS);
    store(&mut h, text("a"));
    store(&mut h, text("b"));
    let ids: Vec<i64> = h
        .list(10)
        .unwrap_or_default()
        .iter()
        .map(|e| e.id)
        .collect();
    assert!(ids.contains(&keep), "the pinned entry is still there");
    assert_eq!(ids.len(), 2, "plus the newest unpinned one");
}

// @D-015
#[test]
fn d015_pinned_cap_100() {
    // The default cap is 100.
    assert_eq!(Limits::default().pinned_cap, 100);
    let (mut h, _) = history_with(Limits {
        pinned_cap: 3,
        ..Limits::default()
    });
    let ids: Vec<i64> = (0..4)
        .map(|i| id_of(&store(&mut h, text(&format!("p{i}")))))
        .collect();
    for id in &ids[..3] {
        assert!(h.pin(*id).is_ok());
    }
    assert!(matches!(h.pin(ids[3]), Err(Error::PinLimit)));
    assert!(
        h.pin(ids[0]).is_ok(),
        "pinning an already pinned entry is fine"
    );
    assert!(h.unpin(ids[0]).is_ok());
    assert!(h.pin(ids[3]).is_ok());
}

// @R-015
#[test]
fn r015_ignored_app_not_stored() {
    let (mut h, _) = history();
    for owner in ["Proton Pass.exe", "proton pass.EXE"] {
        let mut c = text("hunter2");
        c.owner_exe = Some(owner.into());
        assert_eq!(store(&mut h, c).result, Err(Skip::IgnoredApp), "{owner}");
    }
    let mut c = text("fine");
    c.owner_exe = Some("firefox.exe".into());
    assert!(store(&mut h, c).result.is_ok());
    let mut unknown = text("no owner");
    unknown.owner_exe = None;
    assert!(store(&mut h, unknown).result.is_ok());
}

// @D-015
#[test]
fn d015_oversize_text_rejected_and_file_lists_are_capped() {
    let (mut h, _) = history();
    let big = "x".repeat(MAX_TEXT_BYTES + 1);
    assert_eq!(store(&mut h, text(&big)).result, Err(Skip::TooLarge));
    assert!(store(&mut h, text(&"x".repeat(MAX_TEXT_BYTES)))
        .result
        .is_ok());
    let paths: Vec<String> = (0..1500).map(|i| format!(r"C:\data\file{i}.txt")).collect();
    let id = id_of(&store(&mut h, cap(Content::Files(paths))));
    let list = h.payload(id).ok().flatten().unwrap_or_default();
    assert_eq!(String::from_utf8_lossy(&list).lines().count(), 1000);
    assert_eq!(store(&mut h, text("   \n ")).result, Err(Skip::Empty));
}

// @D-015
#[test]
fn d015_store_cap_evicts_oldest_unpinned() {
    let (mut h, clock) = history_with(Limits {
        store_cap_bytes: 250,
        ..Limits::default()
    });
    let first = id_of(&store(&mut h, text(&"a".repeat(100))));
    clock.advance(1000);
    let pinned = id_of(&store(&mut h, text(&"b".repeat(100))));
    assert!(h.pin(pinned).is_ok());
    clock.advance(1000);
    let o = store(&mut h, text(&"c".repeat(100)));
    assert_eq!(
        o.purged.evicted, 1,
        "300 bytes over a 250 cap: one eviction"
    );
    let ids: Vec<i64> = h
        .list(10)
        .unwrap_or_default()
        .iter()
        .map(|e| e.id)
        .collect();
    assert!(!ids.contains(&first), "the oldest unpinned went");
    assert!(ids.contains(&pinned), "the pinned one stayed");
    assert!(ids.contains(&id_of(&o)), "the new one stayed");
}

// @D-015
#[test]
fn d015_capture_rate_limited() {
    let mut l = Limiter::default();
    for i in 0..10 {
        assert_eq!(l.admit(i * 10), Ok(()), "capture {i}");
    }
    // The 11th inside the second waits until the oldest one is a second old.
    assert_eq!(l.admit(100), Err(900));
    assert_eq!(l.admit(999), Err(1));
    // Trailing edge: after the wait the next capture runs, so the last state is the one stored.
    assert_eq!(l.admit(1000), Ok(()));
}

struct FakeSource {
    seq: Cell<u32>,
    bump_on_read: bool,
    content: Captured,
}

impl ClipSource for FakeSource {
    fn sequence(&self) -> u32 {
        self.seq.get()
    }
    fn read(&self) -> Option<Captured> {
        if self.bump_on_read {
            self.seq.set(self.seq.get() + 1);
        }
        Some(self.content.clone())
    }
}

// @D-014
#[test]
fn d014_capture_discards_read_if_sequence_changed() {
    let (mut h, _) = history();
    let mut src = FakeSource {
        seq: Cell::new(7),
        bump_on_read: true,
        content: text("half-written"),
    };
    assert_eq!(capture(&src, 7, &mut h).ok(), Some(Capture::Discarded));
    assert_eq!(count(&h), 0);
    src.bump_on_read = false;
    let seq = src.seq.get();
    assert!(matches!(
        capture(&src, seq, &mut h),
        Ok(Capture::Decided(_))
    ));
    assert_eq!(count(&h), 1);
}

// @D-019
#[test]
fn d019_marker_writes_never_stored() {
    let (mut h, _) = history();
    let mut c = text("copied by the launcher");
    c.marker = true;
    assert_eq!(store(&mut h, c).result, Err(Skip::Marker));
    assert_eq!(count(&h), 0);
}

// @D-014
#[test]
fn d014_a_repeated_copy_refreshes_the_latest_entry() {
    let (mut h, clock) = history();
    let a = id_of(&store(&mut h, text("same")));
    clock.advance(5000);
    let again = store(&mut h, text("same"));
    assert_eq!(again.result, Ok(Stored::Refreshed(a)));
    assert_eq!(count(&h), 1);
    store(&mut h, text("other"));
    assert!(matches!(
        store(&mut h, text("same")).result,
        Ok(Stored::New(_))
    ));
}

// @D-014
#[test]
fn d014_payload_buffers_zeroed_on_drop() {
    let (mut h, _) = history();
    let id = id_of(&store(&mut h, text("wipe me")));
    // The type is the guarantee: Zeroizing wipes the buffer when it is dropped.
    let payload: Option<Zeroizing<Vec<u8>>> = h.payload(id).ok().flatten();
    let preview: Option<Zeroizing<String>> = h.preview(id).ok().flatten();
    assert!(payload.is_some() && preview.is_some());
}

// @D-014
#[test]
fn d014_text_index_dropped_after_60s() {
    let (mut h, clock) = history();
    let a = id_of(&store(&mut h, text("Hello World")));
    store(&mut h, text("another"));
    assert!(!h.index_loaded());
    assert_eq!(h.search("WORLD").ok(), Some(vec![a]), "case-insensitive");
    assert!(h.index_loaded());
    h.view_closed();
    clock.advance(INDEX_TTL_MS - 1);
    h.tick();
    assert!(h.index_loaded(), "not yet");
    clock.advance(1);
    h.tick();
    assert!(!h.index_loaded(), "dropped 60 s after the view closed");
    // Opening the view again within the window keeps it.
    assert_eq!(h.search("another").ok().map(|v| v.len()), Some(1));
    h.view_closed();
    clock.advance(INDEX_TTL_MS / 2);
    assert!(h.search("hello").is_ok());
    clock.advance(INDEX_TTL_MS);
    h.tick();
    assert!(
        h.index_loaded(),
        "a search reopened the view, so no drop is pending"
    );
}
