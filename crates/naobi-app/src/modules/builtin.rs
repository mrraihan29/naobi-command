//! The built-in rows (NCMD-18): system commands, quicklinks and the calculator. The commands run
//! through `AppOps`, which logs each one by id (T-009) and calls Windows.
use super::clipboard_capture::DpapiCipher;
use super::Modules;
use crate::logging;
use naobi_core::db::Db;
use naobi_core::quicklink::Quicklink;
use naobi_launcher::providers::calc::CalcSource;
use naobi_launcher::providers::color::ColorSource;
use naobi_launcher::providers::quicklink::{QuicklinkSet, QuicklinkSource};
use naobi_launcher::providers::run::{Program, ProgramLookup, RunSource};
use naobi_launcher::providers::system::{SystemCommand, SystemOps, SystemSource};
use naobi_launcher::run_history::RunHistory;
use naobi_win32::{platform, random, run, shell_exec, system_commands};
use std::sync::{Arc, OnceLock};

/// The quicklinks of the live settings: the settings load and every hot reload `set` them.
fn quicklinks() -> &'static QuicklinkSet {
    static SET: OnceLock<QuicklinkSet> = OnceLock::new();
    SET.get_or_init(QuicklinkSet::default)
}

pub fn set_quicklinks(links: Vec<Quicklink>) {
    quicklinks().set(links);
}

pub fn register(m: &mut Modules) {
    m.register_source(Box::new(CalcSource::with_now(now_ms)));
    m.register_source(Box::new(ColorSource));
    m.register_source(Box::new(QuicklinkSource::new(quicklinks().clone())));
    m.register_source(Box::new(SystemSource::new(Arc::new(AppOps))));
    m.register_source(Box::new(RunSource::new(
        open_run_history(),
        Box::new(WinLookup),
        Box::new(now_ms),
    )));
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The run history on its own connection to the data file (D-038). None when the store cannot be
/// opened: `>` still runs lines, without history.
fn open_run_history() -> Option<RunHistory> {
    let opened = (|| {
        let path = naobi_core::paths::data_dir()?.join("naobi.db");
        std::fs::create_dir_all(path.parent()?).ok()?;
        let seed = random::random16().ok()?;
        let db = Db::open(&path, move || seed).ok()?;
        RunHistory::open(db, Box::new(DpapiCipher), now_ms()).ok()
    })();
    if opened.is_none() {
        tracing::warn!("the run history is off: the store could not be opened");
    }
    opened
}

/// The first token of a `>` line against PATH and App Paths (T-035: no network paths).
struct WinLookup;

impl ProgramLookup for WinLookup {
    fn find(&self, token: &str) -> Option<Program> {
        run::find_program(token).map(|f| Program {
            path: f.path,
            gui: f.gui,
        })
    }
}

struct AppOps;

/// A Windows failure is logged with its system error; the Notice stays a plain sentence.
fn windows_failed(command: SystemCommand, e: &std::io::Error) -> String {
    tracing::warn!(command = command.id(), error = %e, "system command failed");
    "Windows refused the command.".to_owned()
}

impl SystemOps for AppOps {
    fn run(&self, command: SystemCommand) -> Result<(), String> {
        logging::system_command(command.id());
        let done = |r: std::io::Result<()>| r.map_err(|e| windows_failed(command, &e));
        match command {
            SystemCommand::Lock => done(system_commands::lock()),
            SystemCommand::Sleep => done(system_commands::suspend(false)),
            SystemCommand::Hibernate => done(system_commands::suspend(true)),
            SystemCommand::Restart => done(system_commands::restart()),
            SystemCommand::ShutDown => done(system_commands::shut_down()),
            SystemCommand::SignOut => done(system_commands::sign_out()),
            SystemCommand::EmptyRecycleBin => done(system_commands::empty_recycle_bin()),
            SystemCommand::ToggleDarkMode => done(system_commands::toggle_dark_mode()),
            SystemCommand::OpenWindowsSettings => shell_exec::open("ms-settings:")
                .map_err(|_| "Windows could not open Settings.".to_owned()),
            SystemCommand::OpenSettingsFile => platform::open_settings()
                .then_some(())
                .ok_or_else(|| "Windows could not open the settings file.".to_owned()),
            // Resuming is in the tray menu: the Launcher hotkey is off while they are paused.
            SystemCommand::PauseHotkeys => platform::set_hotkeys_paused(true)
                .then_some(())
                .ok_or_else(|| "The hotkeys could not be paused.".to_owned()),
            SystemCommand::Quit => platform::quit()
                .then_some(())
                .ok_or_else(|| "The app could not quit.".to_owned()),
        }
    }
}
