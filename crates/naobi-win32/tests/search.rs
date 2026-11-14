//! Tier 2: the real Windows Search index. The test takes a text file the index already knows, then
//! finds it again by its name with the ranked query the Launcher uses. It needs the `WSearch`
//! service to run and at least one indexed .txt file; a fresh file would only prove how fast the
//! indexer is.
use naobi_win32::search::{query, service_running};

// @R-013
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r013_indexed_file_appears_by_name() {
    assert!(
        service_running(),
        "the Windows Search service is not running"
    );
    let known = query(
        "SELECT TOP 50 System.ItemPathDisplay FROM SystemIndex \
         WHERE System.FileName LIKE '%.txt' AND scope='file:'",
    )
    .unwrap_or_else(|e| panic!("the sample query failed: {e:?}"));
    // A name without quotes keeps the hand-written query below simple.
    let Some(path) = known.iter().find(|p| !p.contains(['\'', '"'])) else {
        panic!("no indexed .txt file to look for: {known:?}");
    };
    let name = path.rsplit('\\').next().unwrap_or(path);
    let template = "SELECT TOP 30 System.ItemPathDisplay FROM SystemIndex WHERE CONTAINS(System.FileName, '\"NAME\"') AND scope='file:' ORDER BY System.Search.Rank DESC";
    let sql = template.replace("NAME", name);
    let found = query(&sql).unwrap_or_else(|e| panic!("the name query failed: {e:?}"));
    assert!(
        found.iter().any(|p| p.eq_ignore_ascii_case(path)),
        "{path} not among {found:?}"
    );
}

// @R-013
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r013_a_malformed_query_fails_without_taking_the_process_down() {
    assert!(query("SELECT nonsense FROM nowhere WHERE").is_err());
}
