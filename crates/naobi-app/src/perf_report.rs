//! Readiness lines for the latency item (feature `perf`): one JSON object per line on stdout.

/// `{"ev":"tray_ready","qpc":123}`; `qpc` is the QueryPerformanceCounter tick count.
pub fn line(ev: &str, qpc: i64) -> String {
    serde_json::json!({ "ev": ev, "qpc": qpc }).to_string()
}

pub fn emit(ev: &str) {
    println!("{}", line(ev, naobi_win32::platform::qpc()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn readiness_line_has_event_and_qpc() {
        assert_eq!(
            super::line("tray_ready", 42),
            r#"{"ev":"tray_ready","qpc":42}"#
        );
    }
}
