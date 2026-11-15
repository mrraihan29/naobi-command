//! Tier 2 (interactive desktop, dogfood instance quit): typing the name of a file the Windows Search
//! index knows shows the Files section in the Launcher. The name comes from the index itself, so the
//! test does not depend on how fast the indexer is. The app prints `files_shown` (feature `perf`)
//! when a snapshot with a Files section is posted.
use naobi_win32::search::query;
use naobi_win32::{autostart, dwm, inject};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

struct RunKey(String);

impl Drop for RunKey {
    fn drop(&mut self) {
        autostart::remove_key_tree(&self.0);
    }
}

// @R-013
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r013_typing_an_indexed_file_name_shows_the_files_section() {
    let known = query(
        "SELECT TOP 50 System.ItemPathDisplay FROM SystemIndex \
         WHERE System.FileName LIKE '%.txt' AND scope='file:'",
    )
    .unwrap_or_else(|e| panic!("the sample query failed: {e:?}"));
    // A plain name: letters, digits and dots, long enough to be rare.
    let Some(name) = known
        .iter()
        .filter_map(|p| p.rsplit('\\').next())
        .find(|n| n.len() > 8 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'))
    else {
        panic!("no indexed .txt file with a plain name: {known:?}");
    };
    let dir = std::env::temp_dir().join(format!("naobi-flow-files-{}", std::process::id()));
    let run_key = RunKey(format!(
        r"Software\NaobiCommandTest\flow-files-{}",
        std::process::id()
    ));
    let mut app = match Command::new(env!("CARGO_BIN_EXE_naobi-app"))
        .env("NAOBI_TEST_RUN_KEY", &run_key.0)
        .env("NAOBI_PERF_ITEMS", "0")
        .env("NAOBI_DATA_DIR", dir.join("data"))
        .env("NAOBI_SETTINGS_DIR", dir.join("settings"))
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => panic!("spawn naobi-app: {e}"),
    };
    let (tx, lines) = mpsc::channel();
    if let Some(out) = app.stdout.take() {
        std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
    }
    let ready = until(Duration::from_secs(5), || {
        dwm::find_window(naobi_ui::TITLE).is_some()
    });
    std::thread::sleep(Duration::from_millis(800));
    inject::launcher();
    let opened = until(Duration::from_secs(3), || {
        dwm::find_window(naobi_ui::TITLE).is_some_and(|h| !dwm::is_cloaked(h))
    });
    std::thread::sleep(Duration::from_millis(400));
    inject::text(name);
    let mut shown = false;
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end && !shown {
        shown = lines
            .recv_timeout(Duration::from_millis(200))
            .is_ok_and(|l| l.contains("files_shown"));
    }
    let _ = app.kill();
    let _ = app.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready, "launcher window never appeared");
    assert!(opened, "the Launcher did not open");
    assert!(shown, "no Files section for {name}");
}

// @R-013
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r013_the_launchers_own_queries_run_against_the_real_index() {
    use naobi_launcher::providers::files::{contains_sql, like_sql};
    let known = query(
        "SELECT TOP 50 System.ItemPathDisplay FROM SystemIndex \
         WHERE System.FileName LIKE '%.txt' AND scope='file:'",
    )
    .unwrap_or_else(|e| panic!("the sample query failed: {e:?}"));
    let Some(path) = known.iter().find(|p| {
        p.rsplit('\\').next().is_some_and(|n| {
            n.len() > 8 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
        })
    }) else {
        panic!("no indexed .txt file with a plain name: {known:?}");
    };
    let name = path.rsplit('\\').next().unwrap_or(path);
    // The ranked query by the start of the name, and the fallback by a piece from its middle.
    let start = &name[..name.len().min(6)];
    let ranked = contains_sql(start, 30).unwrap_or_default();
    let found = query(&ranked).unwrap_or_else(|e| panic!("ranked query failed: {e:?}"));
    assert!(
        found.iter().any(|p| p.eq_ignore_ascii_case(path)),
        "{path} not among {found:?} for {start}"
    );
    let middle = &name[2..name.len() - 2];
    let any = like_sql(middle, 30).unwrap_or_default();
    let found = query(&any).unwrap_or_else(|e| panic!("LIKE query failed: {e:?}"));
    assert!(
        found.iter().any(|p| p.eq_ignore_ascii_case(path)),
        "{path} not among {found:?} for {middle}"
    );
    // Wildcards and quotes typed by the user are plain characters: the substring query always runs.
    // (The ranked one may be refused for punctuation; the child then uses the substring query.)
    for text in ["50%", "a_b", "[x]", "o'brien", "\"quoted\"", "x' OR 1=1 --"] {
        if let Some(sql) = like_sql(text, 30) {
            assert!(query(&sql).is_ok(), "{text}: {sql}");
        }
    }
}
