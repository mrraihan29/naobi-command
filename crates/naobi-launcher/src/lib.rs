//! Root search engine: ranking, frecency, view stack, Esc ladder and armed commands.
//! A functional core: `Launcher::handle` returns `Effect`s, and nothing here touches Win32 or Slint.
#![forbid(unsafe_code)]
pub mod armed;
pub mod calc_history;
pub mod engine;
pub mod esc;
pub mod file_index;
pub mod frecency;
pub mod icons;
pub mod ports;
pub mod providers;
pub mod rank;
pub mod run_history;

pub use engine::Launcher;
pub use ports::Clock;
